use crate::card_backs::CardBack;
use crate::error::{ProxyNexusError, Result};
use crate::image_provider::ImageProvider;
use crate::models::{BleedPreference, Printing, SourceImage, expand_to_cards};
use crate::print_prep;
use image::ImageFormat;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Seek, Write};
use tracing::info;
use web_time::Instant;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Cardstock {
    S27,
    #[default]
    S30,
    S33,
    M31,
    P10,
}

impl Cardstock {
    pub const ALL: [Cardstock; 5] = [
        Cardstock::S27,
        Cardstock::S30,
        Cardstock::S33,
        Cardstock::M31,
        Cardstock::P10,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Cardstock::S27 => "(S27) Smooth",
            Cardstock::S30 => "(S30) Standard Smooth",
            Cardstock::S33 => "(S33) Superior Smooth",
            Cardstock::M31 => "(M31) Linen",
            Cardstock::P10 => "(P10) Plastic",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct MpcOptions {
    pub upscale: bool,
    pub autofill: bool,
    pub back_label: Option<&'static str>,
    pub cardstock: Cardstock,
}

#[derive(Default)]
struct OrderPlan {
    fronts: Vec<(String, usize)>,
    backs: Vec<(String, usize)>,
    shared: Vec<(&'static str, usize)>,
}

pub async fn generate_mpc_zip(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    options: MpcOptions,
    card_backs: Vec<(&'static CardBack, Vec<u8>)>,
    progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<u8>> {
    // A printing with several backs is several cards sharing one front, so the
    // card count is not the printing count. `expand_to_cards` settles both.
    let (quantity, total_images) = {
        let cards = expand_to_cards(&printings);
        let images = cards
            .iter()
            .map(|(_, card)| 1 + usize::from(card.back.is_some()))
            .sum();
        (cards.len(), images)
    };
    let mut processed_images = 0;

    let mut back_groups: BTreeMap<String, Vec<Printing>> = BTreeMap::new();
    for printing in printings {
        back_groups
            .entry(printing.back_group.clone())
            .or_default()
            .push(printing);
    }

    let mut zip_buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut zip_buffer);

    let single_group = back_groups.len() == 1;
    let mut plan = OrderPlan::default();
    let mut slot_offset = 0;

    for (group_name, group_printings) in back_groups {
        let folder_name = if single_group {
            "card-images".to_string()
        } else {
            format!("{}-images", group_name)
        };

        let shared_back = options.back_label.and_then(|label| {
            card_backs
                .iter()
                .map(|(back, _)| *back)
                .find(|back| back.back_group == group_name && back.label == label)
        });
        let group_size = {
            let cards = expand_to_cards(&group_printings);
            if let Some(back) = shared_back {
                for (position, (_, card)) in cards.iter().enumerate() {
                    if card.back.is_none() {
                        plan.shared.push((back.file, slot_offset + position));
                    }
                }
            }
            cards.len()
        };

        let group_start = slot_offset;
        slot_offset += group_size;

        process_back_group(
            group_printings,
            image_provider,
            options,
            &mut zip,
            &folder_name,
            &progress,
            &mut processed_images,
            total_images,
            group_start,
            &mut plan,
        )
        .await?;
    }

    let file_options =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (back, bytes) in card_backs {
        let prepared = if back.has_bleed {
            bytes
        } else {
            let format = image::guess_format(&bytes).unwrap_or(ImageFormat::Jpeg);
            let img = image::load_from_memory(&bytes)?;
            print_prep::encode_image(print_prep::add_mpc_bleed_border(&img), format)?
        };

        zip.start_file(back.file, file_options)?;
        zip.write_all(&prepared)?;
    }

    if options.autofill {
        if plan.backs.is_empty() && plan.shared.is_empty() {
            return Err(ProxyNexusError::Internal(
                "No card has a back image, so order.xml cannot be written. Every \
                 MakePlayingCards card needs a back."
                    .to_string(),
            ));
        }

        zip.start_file("order.xml", file_options)?;
        zip.write_all(build_order_xml(quantity, options.cardstock, &plan).as_bytes())?;
    }

    zip.finish()?;
    Ok(zip_buffer.into_inner())
}

#[allow(clippy::too_many_arguments)]
async fn process_back_group<W: Write + Seek>(
    printings: Vec<Printing>,
    image_provider: &impl ImageProvider,
    options: MpcOptions,
    zip: &mut ZipWriter<W>,
    folder_name: &str,
    progress: &Option<Box<dyn Fn(f32) + Send + Sync>>,
    processed_images: &mut usize,
    total_images: usize,
    slot_offset: usize,
    plan: &mut OrderPlan,
) -> Result<()> {
    let mut copy_counters: HashMap<(String, String, Option<String>), u32> = HashMap::new();

    struct ImageRequest {
        printing: Printing,
        side_name: String,
        source: SourceImage,
        copy_num: u32,
        slot: usize,
    }

    let mut requests: Vec<ImageRequest> = Vec::new();

    for (position, (_, card)) in expand_to_cards(&printings).iter().enumerate() {
        let slot = slot_offset + position;
        let printing = card.printing;
        let key = (
            printing.collection.clone(),
            printing.card_id.clone(),
            printing.variant.clone(),
        );
        let copy_num = *copy_counters
            .entry(key)
            .and_modify(|n| *n += 1)
            .or_insert(1);

        if let Some(source) = card.front.image(BleedPreference::Bleed) {
            requests.push(ImageRequest {
                printing: printing.clone(),
                side_name: "front".to_string(),
                source,
                copy_num,
                slot,
            });
        }

        if let Some(source) = card
            .back
            .and_then(|back| back.image(BleedPreference::Bleed))
        {
            requests.push(ImageRequest {
                printing: printing.clone(),
                side_name: "back".to_string(),
                source,
                copy_num,
                slot,
            });
        }
    }

    requests.sort_by(|a, b| a.source.key.cmp(&b.source.key));

    struct CachedImage {
        key: String,
        image: image::RgbImage,
        format: ImageFormat,
    }

    let mut current_cache: Option<CachedImage> = None;
    let mut current_file: Option<String> = None;

    for req in requests {
        let printing = req.printing;
        let side_name = req.side_name;
        let current_image_key = req.source.key;
        let copy_num = req.copy_num;

        let start = Instant::now();

        if current_cache
            .as_ref()
            .is_none_or(|c| c.key != current_image_key)
        {
            let mut image_data = image_provider.get_image_bytes(&current_image_key).await?;

            if options.upscale {
                image_data = crate::upscale_image(&image_data).await?
            }

            let image_format = image::guess_format(&image_data).unwrap_or(ImageFormat::Jpeg);
            let img = image::load_from_memory(&image_data)?;
            let bleed_image = if req.source.has_bleed {
                img.to_rgb8()
            } else {
                print_prep::add_mpc_bleed_border(&img)
            };

            current_cache = Some(CachedImage {
                key: current_image_key.clone(),
                image: bleed_image,
                format: image_format,
            });
            current_file = None;
        }

        let cached = current_cache.as_ref().unwrap();

        let ext = if cached.format == ImageFormat::Png {
            "png"
        } else {
            "jpg"
        };

        let variant_label = printing.variant.as_deref().unwrap_or("official");

        let filename = if side_name == "front" {
            format!(
                "{}/{}-{}-{}-{}.{}",
                folder_name, printing.card_id, variant_label, printing.collection, copy_num, ext
            )
        } else {
            format!(
                "{}/{}-{}-{}-{}-{}.{}",
                folder_name,
                printing.card_id,
                variant_label,
                printing.collection,
                copy_num,
                side_name,
                ext
            )
        };

        let path = match &current_file {
            Some(written) if options.autofill => written.clone(),
            _ => {
                let bordered_bytes = print_prep::encode_image(cached.image.clone(), cached.format)?;
                let file_options =
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

                zip.start_file(&filename, file_options)?;
                zip.write_all(&bordered_bytes)?;
                current_file = Some(filename.clone());
                filename
            }
        };

        if side_name == "front" {
            plan.fronts.push((path, req.slot));
        } else {
            plan.backs.push((path, req.slot));
        }

        *processed_images += 1;
        if let Some(cb) = progress
            && total_images > 0
        {
            cb(*processed_images as f32 / total_images as f32);
        }

        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(0).await;

        info!(
            "Runtime for image {}: {:?}",
            current_image_key,
            start.elapsed()
        );
    }

    Ok(())
}

/// Builds the `order.xml` the mpc-autofill desktop tool reads. Paths are
/// relative to the directory the zip is extracted into.
fn build_order_xml(quantity: usize, cardstock: Cardstock, plan: &OrderPlan) -> String {
    let mut shared: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (file, slot) in &plan.shared {
        shared.entry(file).or_default().push(*slot);
    }

    let common = shared
        .iter()
        .max_by_key(|(_, slots)| slots.len())
        .map(|(file, _)| *file);

    let mut xml = String::from("<order>\n  <details>\n");
    xml.push_str(&format!("    <quantity>{}</quantity>\n", quantity));
    xml.push_str(&format!(
        "    <stock>{}</stock>\n",
        escape(cardstock.as_str())
    ));
    xml.push_str("    <foil>false</foil>\n");
    xml.push_str("  </details>\n");

    push_face(&mut xml, "fronts", |out| {
        for (path, slot) in &plan.fronts {
            push_card(out, path, &[*slot]);
        }
    });

    push_face(&mut xml, "backs", |out| {
        for (path, slot) in &plan.backs {
            push_card(out, path, &[*slot]);
        }
        for (file, slots) in &shared {
            if Some(*file) != common {
                push_card(out, file, slots);
            }
        }
    });

    xml.push_str("  <cardback>");
    if let Some(file) = common {
        xml.push_str(&escape(&format!("./{}", file)));
    }
    xml.push_str("</cardback>\n</order>\n");
    xml
}

fn push_face(xml: &mut String, tag: &str, body: impl FnOnce(&mut String)) {
    let mut inner = String::new();
    body(&mut inner);
    if !inner.is_empty() {
        xml.push_str(&format!("  <{}>\n{}  </{}>\n", tag, inner, tag));
    }
}

fn push_card(xml: &mut String, path: &str, slots: &[usize]) {
    let name = path.rsplit('/').next().unwrap_or(path);
    let slots = slots
        .iter()
        .map(|slot| slot.to_string())
        .collect::<Vec<_>>()
        .join(",");

    xml.push_str("    <card>\n");
    xml.push_str(&format!(
        "      <id>{}</id>\n",
        escape(&format!("./{}", path))
    ));
    xml.push_str("      <sourceType>Local File</sourceType>\n");
    xml.push_str(&format!("      <slots>{}</slots>\n", slots));
    xml.push_str(&format!("      <name>{}</name>\n", escape(name)));
    xml.push_str("    </card>\n");
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(quantity: usize, plan: &OrderPlan) -> String {
        build_order_xml(quantity, Cardstock::default(), plan)
    }

    /// The autofill tool matches these against its own list and rejects anything
    /// else, so a typo here only shows up outside this repo.
    #[test]
    fn every_cardstock_is_spelled_the_way_the_autofill_tool_expects() {
        let expected = [
            (Cardstock::S27, "(S27) Smooth"),
            (Cardstock::S30, "(S30) Standard Smooth"),
            (Cardstock::S33, "(S33) Superior Smooth"),
            (Cardstock::M31, "(M31) Linen"),
            (Cardstock::P10, "(P10) Plastic"),
        ];
        for (stock, spelling) in expected {
            assert_eq!(stock.as_str(), spelling, "{:?}", stock);
        }
        assert_eq!(MpcOptions::default().cardstock, Cardstock::S30);
        assert_eq!(
            Cardstock::ALL.len(),
            expected.len(),
            "a stock is missing from ALL"
        );
        assert!(Cardstock::ALL.contains(&MpcOptions::default().cardstock));
    }

    #[test]
    fn each_planned_file_becomes_a_card_naming_its_slot() {
        let plan = OrderPlan {
            fronts: vec![
                ("card-images/hedge_fund-official-sg-1.jpg".into(), 0),
                ("card-images/hedge_fund-official-sg-2.jpg".into(), 1),
            ],
            ..OrderPlan::default()
        };
        let out = xml(2, &plan);

        assert!(out.contains("<quantity>2</quantity>"));
        assert!(out.contains("<id>./card-images/hedge_fund-official-sg-1.jpg</id>"));
        assert!(out.contains("<slots>0</slots>"));
        assert!(out.contains("<slots>1</slots>"));
        assert!(out.contains("<sourceType>Local File</sourceType>"));
    }

    #[test]
    fn the_shared_back_covering_the_most_slots_becomes_the_common_cardback() {
        let plan = OrderPlan {
            shared: vec![
                ("corp-back.png", 0),
                ("corp-back.png", 1),
                ("corp-back.png", 2),
                ("runner-back.png", 3),
            ],
            ..OrderPlan::default()
        };
        let out = xml(4, &plan);

        assert!(out.contains("<cardback>./corp-back.png</cardback>"));
        // The common back is left out of <backs> so the tool fills it into every
        // slot no other back claims, exactly as the frontend's own XML does.
        assert!(!out.contains("./corp-back.png</id>"));
        assert!(out.contains("<id>./runner-back.png</id>"));
        assert!(out.contains("<slots>3</slots>"));
    }

    #[test]
    fn a_cards_own_back_is_listed_against_its_slot_and_leaves_the_cardback_empty() {
        let plan = OrderPlan {
            fronts: vec![("card-images/a-official-x-1.jpg".into(), 0)],
            backs: vec![("card-images/a-official-x-1-back.jpg".into(), 0)],
            ..OrderPlan::default()
        };
        let out = xml(1, &plan);

        assert!(out.contains("<backs>"));
        assert!(out.contains("<id>./card-images/a-official-x-1-back.jpg</id>"));
        // Nothing falls back on a shared back, so there is no common one to name.
        assert!(out.contains("<cardback></cardback>"));
        assert!(out.contains("<foil>false</foil>"));
    }

    #[test]
    fn markup_characters_in_a_path_are_escaped() {
        let plan = OrderPlan {
            fronts: vec![("card-images/a&b-official-x-1.jpg".into(), 0)],
            ..OrderPlan::default()
        };
        let out = xml(1, &plan);

        assert!(out.contains("<id>./card-images/a&amp;b-official-x-1.jpg</id>"));
        assert!(!out.contains("a&b-official"));
    }

    struct FakeImages;

    impl crate::image_provider::ImageProvider for FakeImages {
        async fn get_image_bytes(&self, _key: &str) -> Result<Vec<u8>> {
            let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(744, 1038));
            print_prep::encode_image(img.to_rgb8(), ImageFormat::Jpeg)
        }
    }

    fn side(key: &str) -> crate::models::CardSide {
        crate::models::CardSide {
            image_key: Some(key.to_string()),
            bleed_image_key: None,
        }
    }

    fn a_printing(with_back: bool) -> Printing {
        a_printing_with_backs(if with_back { &["back.jpg"] } else { &[] })
    }

    fn a_printing_with_backs(backs: &[&str]) -> Printing {
        Printing {
            card_id: "hedge_fund".into(),
            card_title: "Hedge Fund".into(),
            is_official: true,
            variant: None,
            front: side("front.jpg"),
            backs: backs.iter().map(|key| side(key)).collect(),
            collection: "nsg".into(),
            back_group: "corp".into(),
            pack_id: None,
            date_release: None,
            position: None,
        }
    }

    async fn generate(autofill: bool, with_back: bool) -> Result<Vec<u8>> {
        generate_mpc_zip(
            vec![
                a_printing(with_back),
                a_printing(with_back),
                a_printing(with_back),
            ],
            &FakeImages,
            MpcOptions {
                autofill,
                ..MpcOptions::default()
            },
            Vec::new(),
            None,
        )
        .await
    }

    async fn zip_entries(autofill: bool, with_back: bool) -> Vec<String> {
        let bytes = generate(autofill, with_back).await.unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect()
    }

    #[tokio::test]
    async fn without_the_xml_every_copy_still_gets_its_own_file() {
        let names = zip_entries(false, true).await;

        assert_eq!(
            names,
            vec![
                "card-images/hedge_fund-official-nsg-1-back.jpg",
                "card-images/hedge_fund-official-nsg-2-back.jpg",
                "card-images/hedge_fund-official-nsg-3-back.jpg",
                "card-images/hedge_fund-official-nsg-1.jpg",
                "card-images/hedge_fund-official-nsg-2.jpg",
                "card-images/hedge_fund-official-nsg-3.jpg",
            ]
        );
    }

    #[tokio::test]
    async fn with_the_xml_repeat_copies_share_one_file() {
        let names = zip_entries(true, true).await;

        assert_eq!(
            names,
            vec![
                "card-images/hedge_fund-official-nsg-1-back.jpg",
                "card-images/hedge_fund-official-nsg-1.jpg",
                "order.xml",
            ]
        );
    }

    #[tokio::test]
    async fn autofilling_cards_that_have_no_back_image_is_an_error() {
        let err = generate(true, false).await.unwrap_err();

        assert!(
            err.to_string().contains("No card has a back image"),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn cards_with_no_back_image_are_fine_when_not_autofilling() {
        let names = zip_entries(false, false).await;

        assert_eq!(names.len(), 3);
        assert!(!names.contains(&"order.xml".to_string()));
    }

    async fn zip_entries_of(printings: Vec<Printing>, autofill: bool) -> Vec<String> {
        let bytes = generate_mpc_zip(
            printings,
            &FakeImages,
            MpcOptions {
                autofill,
                ..MpcOptions::default()
            },
            Vec::new(),
            None,
        )
        .await
        .unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect()
    }

    /// The PDF path prints one card per back, all sharing the front, so MPC has
    /// to lay out the same cards.
    #[tokio::test]
    async fn a_card_with_several_backs_becomes_one_card_per_back() {
        let names = zip_entries_of(
            vec![a_printing_with_backs(&["b1.jpg", "b2.jpg", "b3.jpg"])],
            false,
        )
        .await;

        assert_eq!(
            names,
            vec![
                "card-images/hedge_fund-official-nsg-1-back.jpg",
                "card-images/hedge_fund-official-nsg-2-back.jpg",
                "card-images/hedge_fund-official-nsg-3-back.jpg",
                "card-images/hedge_fund-official-nsg-1.jpg",
                "card-images/hedge_fund-official-nsg-2.jpg",
                "card-images/hedge_fund-official-nsg-3.jpg",
            ],
            "each back needs its own card, and each card its own front"
        );
    }

    #[tokio::test]
    async fn several_backs_take_a_position_each_sharing_one_front_file() {
        let printings = vec![a_printing_with_backs(&["b1.jpg", "b2.jpg", "b3.jpg"])];
        let bytes = generate_mpc_zip(
            printings,
            &FakeImages,
            MpcOptions {
                autofill: true,
                ..MpcOptions::default()
            },
            Vec::new(),
            None,
        )
        .await
        .unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("order.xml").unwrap(), &mut xml)
            .unwrap();

        assert!(xml.contains("<quantity>3</quantity>"), "{}", xml);
        // One front file, named once per position, so it is uploaded once.
        assert_eq!(
            xml.matches("<id>./card-images/hedge_fund-official-nsg-1.jpg</id>")
                .count(),
            3,
            "{}",
            xml
        );
        for position in 1..=3 {
            let back = format!(
                "<id>./card-images/hedge_fund-official-nsg-{}-back.jpg</id>",
                position
            );
            assert!(xml.contains(&back), "missing {}: {}", back, xml);
        }
        for slot in ["<slots>0</slots>", "<slots>1</slots>", "<slots>2</slots>"] {
            assert!(xml.contains(slot), "missing {}: {}", slot, xml);
        }
    }

    /// Copies of a multi-back card still print one of each, as the PDF path does.
    #[tokio::test]
    async fn asking_for_copies_of_a_multi_back_card_still_gives_one_of_each() {
        let printing = a_printing_with_backs(&["b1.jpg", "b2.jpg"]);
        let names = zip_entries_of(vec![printing.clone(), printing.clone(), printing], false).await;

        assert_eq!(
            names.len(),
            4,
            "two fronts and two backs, not six: {:?}",
            names
        );
    }
}

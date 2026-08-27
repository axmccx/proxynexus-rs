use dioxus::prelude::*;
use proxynexus_core::file_naming::back_label;
use proxynexus_core::models::{BleedPreference, CardSide, Printing, expand_to_cards};
use std::collections::HashMap;
use std::rc::Rc;

use super::build_image_url;
use crate::components::variant_selector::VariantSelectorState;

#[derive(Props, Clone, PartialEq)]
pub struct PreviewGridProps {
    pub base_printings: Vec<Printing>,
    pub printings: Vec<Printing>,
    pub available_variants: HashMap<String, Vec<Printing>>,
    pub open_variant_selector: Signal<Option<VariantSelectorState>>,
}

#[component]
pub fn PreviewGrid(props: PreviewGridProps) -> Element {
    let mut open_variant_selector = props.open_variant_selector;
    let mut mounted_elements = use_signal(HashMap::<(String, usize), Rc<MountedData>>::new);

    let printings = props.printings.clone();
    let base_printings = props.base_printings.clone();
    let available_variants = props.available_variants.clone();

    let mut occurrence_tracker = HashMap::<String, usize>::new();

    rsx! {
        div {
            class: "flex flex-wrap gap-4",
            for (printing, base_printing, card_index, front, back) in preview_rows(&printings, &base_printings) {
                {
                    let title_normalized = proxynexus_core::card_store::normalize_title(&printing.card_title);
                    let occurrence = *occurrence_tracker.entry(title_normalized.clone()).or_insert(0);
                    *occurrence_tracker.get_mut(&title_normalized).unwrap() += 1;
                    let identity = (title_normalized.clone(), occurrence);
                    // Only the first card of a printing anchors the variant
                    // picker, so a multi-back card offers one target, not one
                    // per back.
                    let anchors_variants = card_index == 0;

                    let is_open = if let Some(state) = open_variant_selector.read().as_ref() {
                        state.id == identity
                    } else {
                        false
                    };

                    let has_variants = anchors_variants
                        && available_variants.get(&title_normalized).is_some_and(|v| v.len() > 1);
                    let cursor_class = if has_variants { "cursor-pointer" } else { "" };

                    let is_overridden = printing.variant != base_printing.variant
                        || printing.collection != base_printing.collection
                        || printing.pack_id != base_printing.pack_id;

                    let border_bg_class = if is_overridden {
                        "[background:conic-gradient(from_var(--border-angle),var(--color-fuchsia-200)_80%,_var(--color-fuchsia-500)_86%,_var(--color-fuchsia-300)_90%,_var(--color-fuchsia-500)_94%,_var(--color-fuchsia-200))]"
                    } else {
                        "[background:conic-gradient(from_var(--border-angle),var(--color-cyan-200)_80%,_var(--color-cyan-500)_86%,_var(--color-cyan-300)_90%,_var(--color-cyan-500)_94%,_var(--color-cyan-200))]"
                    };

                    rsx! {
                        div {
                            key: "{title_normalized}-{occurrence}-{card_index}-front",
                            class: "relative group w-[160px] md:w-[250px] aspect-[2.5/3.5] shrink-0 transition-transform duration-150 ease-in-out hover:scale-105 hover:z-20 {cursor_class}",
                            onmounted: {
                                let identity = identity.clone();
                                move |evt| {
                                    mounted_elements.write().insert(identity.clone(), evt.data());
                                }
                            },
                            onclick: {
                                let identity = identity.clone();
                                move |evt| {
                                    if has_variants {
                                        evt.stop_propagation();
                                        if is_open {
                                            open_variant_selector.set(None);
                                        } else if let Some(mounted) = mounted_elements.read().get(&identity) {
                                            let mounted = mounted.clone();
                                            let identity = identity.clone();
                                            spawn(async move {
                                                if let Ok(rect) = mounted.get_client_rect().await {
                                                    open_variant_selector.set(Some(VariantSelectorState {
                                                        id: identity,
                                                        rect: (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height),
                                                    }));
                                                }
                                            });
                                        }
                                    }
                                }
                            },

                            if has_variants {
                                div {
                                    class: "absolute -inset-1 rounded-lg {border_bg_class} animate-border"
                                }
                            }

                            div {
                                class: "relative w-full h-full shadow-lg bg-gray-400 overflow-hidden flex items-center justify-center",
                                {
                                    match front.image(BleedPreference::NoBleed) {
                                        Some(source) => {
                                            let style = if source.has_bleed {
                                                "width: 109.6774%; height: 106.9364%; max-width: none; flex-shrink: 0; image-rendering: auto; -webkit-backface-visibility: hidden;"
                                            } else {
                                                "width: 100%; height: 100%; object-fit: cover; image-rendering: auto; -webkit-backface-visibility: hidden; transform: translateZ(0);"
                                            };
                                            rsx! {
                                                img {
                                                    src: "{build_image_url(&source.key)}",
                                                    crossorigin: "anonymous",
                                                    style: "{style}",
                                                    alt: "{printing.card_title}",
                                                }
                                            }
                                        }
                                        None => rsx! {},
                                    }
                                }
                            }
                        }
                        if let Some(back) = back {
                            div {
                                key: "{title_normalized}-{occurrence}-{card_index}-back",
                                class: "relative group w-[160px] md:w-[250px] aspect-[2.5/3.5] shrink-0 transition-transform duration-150 ease-in-out hover:scale-105 hover:z-20",

                                div {
                                    class: "relative w-full h-full overflow-hidden shadow-lg bg-gray-400 flex items-center justify-center",
                                    {
                                        match back.image(BleedPreference::NoBleed) {
                                            Some(source) => {
                                                let style = if source.has_bleed {
                                                    "width: 109.6774%; height: 106.9364%; max-width: none; flex-shrink: 0; image-rendering: auto; -webkit-backface-visibility: hidden;"
                                                } else {
                                                    "width: 100%; height: 100%; object-fit: cover; image-rendering: auto; -webkit-backface-visibility: hidden; transform: translateZ(0);"
                                                };
                                                rsx! {
                                                    img {
                                                        src: "{build_image_url(&source.key)}",
                                                        crossorigin: "anonymous",
                                                        style: "{style}",
                                                        alt: "{printing.card_title} ({back_label(card_index as u32 + 1)})",
                                                    }
                                                }
                                            }
                                            None => rsx! {},
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The cards the export will print, paired with the base printing each was
/// resolved against, and cloned so the render owns them.
#[allow(clippy::type_complexity)]
fn preview_rows(
    printings: &[Printing],
    base_printings: &[Printing],
) -> Vec<(Printing, Printing, usize, CardSide, Option<CardSide>)> {
    let mut seen = HashMap::<usize, usize>::new();

    expand_to_cards(printings)
        .into_iter()
        .map(|(index, card)| {
            let card_index = seen.entry(index).or_insert(0);
            let row = (
                card.printing.clone(),
                base_printings.get(index).unwrap_or(card.printing).clone(),
                *card_index,
                card.front.clone(),
                card.back.cloned(),
            );
            *card_index += 1;
            row
        })
        .collect()
}

use dioxus::prelude::*;
use proxynexus_core::models::Printing;
use std::collections::HashMap;

fn build_image_url(image_key: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        format!("proxynexus://collections/{}", image_key)
    }

    #[cfg(feature = "web")]
    {
        format!("https://collections.proxynexus.net/{}", image_key)
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PreviewGridProps {
    pub printings: Vec<Printing>,
    pub available_variants: HashMap<String, Vec<Printing>>,
    pub card_counts: HashMap<String, usize>,
    pub on_override: EventHandler<(usize, bool, String, String)>,
}

#[component]
pub fn PreviewGrid(props: PreviewGridProps) -> Element {
    let mut open_popover = use_signal(|| None::<(String, usize)>);

    let printings = props.printings.clone();
    let available_variants = props.available_variants.clone();
    let card_counts = props.card_counts.clone();

    let mut occurrence_tracker = HashMap::<String, usize>::new();

    rsx! {
        div {
            class: "flex flex-wrap gap-4",
            for printing in printings.into_iter() {
                {
                    let title_normalized = proxynexus_core::card_store::normalize_title(&printing.card_title);
                    let occurrence = *occurrence_tracker.entry(title_normalized.clone()).or_insert(0);
                    *occurrence_tracker.get_mut(&title_normalized).unwrap() += 1;
                    let identity = (title_normalized.clone(), occurrence);

                    let is_open = open_popover() == Some(identity.clone());
                    let z_class = if is_open { "z-50" } else { "" };

                    rsx! {
                        div {
                            key: "{title_normalized}-{occurrence}-front",
                            class: "relative group w-[200px] shadow-lg aspect-[2.5/3.5] bg-gray-400 shrink-0 {z_class}",

                            div {
                                class: "w-full h-full overflow-hidden",
                                img {
                                    src: "{build_image_url(&printing.image_key)}",
                                    crossorigin: "anonymous",
                                    class: "w-full h-full object-cover",
                                    alt: "{printing.card_title}",
                                }
                            }

                            if let Some(variants) = available_variants.get(&title_normalized) {
                                if variants.len() > 1 {
                                    button {
                                        class: "absolute top-2 right-2 p-1.5 bg-gray-900 bg-opacity-70 hover:bg-opacity-90 text-white rounded-md opacity-0 group-hover:opacity-100 transition-opacity z-10",
                                        onclick: {
                                            let identity = identity.clone();
                                            move |_| {
                                                if open_popover() == Some(identity.clone()) {
                                                    open_popover.set(None);
                                                } else {
                                                    open_popover.set(Some(identity.clone()));
                                                }
                                            }
                                        },
                                        title: "Change Variant",
                                        svg {
                                            xmlns: "http://www.w3.org/2000/svg",
                                            fill: "none",
                                            view_box: "0 0 24 24",
                                            stroke_width: "1.5",
                                            stroke: "currentColor",
                                            class: "w-5 h-5",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                d: "M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182m0-4.991v4.99"
                                            }
                                        }
                                    }

                                    if open_popover() == Some(identity.clone()) {
                                        PopoverMenu {
                                            printing: printing.clone(),
                                            variants: variants.clone(),
                                            occurrence,
                                            total_copies: *card_counts.get(&title_normalized).unwrap_or(&1),
                                            on_close: move |_| open_popover.set(None),
                                            on_override: {
                                                let title = printing.card_title.clone();
                                                let on_override = props.on_override;
                                                move |(apply_to_all, variant_str): (bool, String)| {
                                                    on_override.call((occurrence, apply_to_all, title.clone(), variant_str));
                                                    if apply_to_all {
                                                        open_popover.set(None);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        for (part_index, part) in printing.parts.iter().enumerate() {
                            div {
                                key: "{title_normalized}-{occurrence}-{part_index}",
                                class: "w-[200px] overflow-hidden shadow-lg aspect-[2.5/3.5] bg-gray-400 opacity-90 border-2 border-dashed border-gray-400 shrink-0",
                                img {
                                    src: "{build_image_url(&part.image_key)}",
                                    crossorigin: "anonymous",
                                    class: "w-full h-full object-cover",
                                    alt: "{printing.card_title} ({part.name})",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PopoverMenuProps {
    printing: Printing,
    variants: Vec<Printing>,
    occurrence: usize,
    total_copies: usize,
    on_close: EventHandler<()>,
    on_override: EventHandler<(bool, String)>,
}

#[component]
fn PopoverMenu(props: PopoverMenuProps) -> Element {
    let mut selected_variant_str = use_signal(|| None::<String>);
    let variants = props.variants.clone();
    let current_variant_str = format!(
        "{}:{}:{}",
        props.printing.variant, props.printing.collection, props.printing.pack_code
    );

    rsx! {
        div {
            class: "absolute top-0 left-full ml-2 bg-white rounded-lg shadow-2xl border border-gray-200 z-50 p-4 flex flex-col gap-3 max-w-[400px]",

            div { class: "flex justify-between items-center",
                h3 { class: "text-sm font-bold text-gray-800", "Select Variant" }
                button {
                    class: "text-gray-400 hover:text-gray-600",
                    onclick: move |_| props.on_close.call(()),
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke_width: "2",
                        stroke: "currentColor",
                        class: "w-4 h-4",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M6 18L18 6M6 6l12 12"
                        }
                    }
                }
            }

            div {
                class: "flex gap-2 overflow-x-auto pb-2 scrollbar-thin scrollbar-thumb-gray-300",
                for v in variants.into_iter() {
                    {
                        let v_str = format!("{}:{}:{}", v.variant, v.collection, v.pack_code);
                        let is_selected = current_variant_str == v_str;

                        rsx! {
                            button {
                                class: format!("relative w-[80px] shrink-0 rounded overflow-hidden aspect-[2.5/3.5] border-2 transition-all {}",
                                    if is_selected {
                                        "border-blue-500 shadow-md ring-2 ring-blue-500 ring-offset-1"
                                    } else {
                                        "border-transparent hover:border-gray-400"
                                    }
                                ),
                                title: "{v.variant} ({v.collection})",
                                onclick: {
                                    let v_str = v_str.clone();
                                    move |_| {
                                        props.on_override.call((false, v_str.clone()));
                                        selected_variant_str.set(Some(v_str.clone()));

                                        if props.total_copies <= 1 {
                                            props.on_close.call(());
                                        }
                                    }
                                },
                                img {
                                    src: "{build_image_url(&v.image_key)}",
                                    crossorigin: "anonymous",
                                    class: "w-full h-full object-cover",
                                    alt: "{v.variant}",
                                }
                            }
                        }
                    }
                }
            }

            if let Some(v_str) = selected_variant_str() {
                if props.total_copies > 1 {
                    div {
                        class: "mt-2 pt-3 border-t border-gray-100 flex flex-col gap-2 animate-fade-in",
                        div { class: "text-sm text-gray-700 font-medium", "Variant applied." }
                        button {
                            class: "w-full py-1.5 px-4 bg-gray-100 hover:bg-gray-200 text-gray-800 text-sm font-semibold rounded-md shadow-sm transition-colors border border-gray-300",
                            onclick: move |_| {
                                props.on_override.call((true, v_str.clone()));
                            },
                            "Apply to all {props.total_copies} copies"
                        }
                    }
                }
            }
        }
    }
}

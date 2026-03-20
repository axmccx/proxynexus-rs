use dioxus::prelude::*;
use proxynexus_core::models::Printing;

use super::build_image_url;

#[derive(Clone, PartialEq)]
pub struct VariantSelectorState {
    pub id: (String, usize),
    pub rect: (f64, f64, f64, f64),
}

#[derive(Props, Clone, PartialEq)]
pub struct VariantSelectorProps {
    pub printing: Printing,
    pub variants: Vec<Printing>,
    pub total_copies: usize,
    pub on_close: EventHandler<()>,
    pub on_override: EventHandler<(bool, String)>,
}

#[component]
pub fn VariantSelector(props: VariantSelectorProps) -> Element {
    let mut selected_variant_str = use_signal(|| None::<String>);
    let variants = props.variants.clone();
    let current_variant_str = format!(
        "{}:{}:{}",
        props.printing.variant, props.printing.collection, props.printing.pack_code
    );

    rsx! {
        div {
            class: "bg-white rounded-lg shadow-2xl border border-gray-200 p-4 flex flex-col gap-3 max-w-[400px]",

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

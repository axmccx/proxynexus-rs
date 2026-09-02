use dioxus::prelude::*;

pub const MPC_AUTOFILL_URL: &str = "https://github.com/chilli-axe/mpc-autofill/wiki/Desktop-Tool";

#[derive(Props, Clone, PartialEq)]
pub struct AutofillInfoProps {
    pub on_close: EventHandler<()>,
    pub pos: (f64, f64, f64),
}

#[component]
pub fn AutofillInfo(props: AutofillInfoProps) -> Element {
    let (x, y, w) = props.pos;

    rsx! {
        div {
            class: "fixed inset-0 z-[2000]",
            onclick: move |_| props.on_close.call(()),

            div {
                class: "absolute max-md:!fixed max-md:!top-1/2 max-md:!left-1/2 max-md:![transform:translate(-50%,-50%)] bg-white p-6 rounded-lg shadow-2xl border border-gray-200 w-[90vw] md:w-96 select-text",
                style: "top: {y - 12.0}px; left: {x + w / 2.0}px; transform: translate(-50%, -100%);",
                onclick: move |evt| evt.stop_propagation(),

                button {
                    class: "absolute top-4 right-4 text-gray-400 hover:text-gray-600 focus:outline-none transition-colors",
                    onclick: move |_| props.on_close.call(()),
                    svg {
                        class: "w-5 h-5",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M6 18L18 6M6 6l12 12" }
                    }
                }

                div { class: "flex flex-col gap-4 text-sm mt-2",
                    div {
                        h4 { class: "font-semibold mb-1", "Manual" }
                        p { class: "text-gray-600 leading-relaxed",
                            "Requires you to upload the images and setup the order yourself. Refer to the steps on the instructions page."
                        }
                    }
                    div {
                        h4 { class: "font-semibold mb-1", "MPC Autofill" }
                        p { class: "text-gray-600 leading-relaxed",
                            "Adds an "
                            code { class: "text-xs bg-gray-100 px-1 py-0.5 rounded", "order.xml" }
                            " to the zip file, in the format used by the "
                            a {
                                href: MPC_AUTOFILL_URL,
                                target: "_blank",
                                class: "text-blue-500 hover:text-blue-700 hover:underline",
                                "mpc-autofill desktop tool"
                            }
                            ", which uploads and places every images for you."
                        }
                    }
                }
            }
        }
    }
}

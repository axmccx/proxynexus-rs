use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AboutModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn AboutModal(props: AboutModalProps) -> Element {
    let instructions_href = if cfg!(target_arch = "wasm32") {
        "/instructions.html".to_string()
    } else {
        "https://proxynexus.net/instructions.html".to_string()
    };

    rsx! {
        div {
            class: "fixed inset-0 flex items-center justify-center z-[2000]",
            style: "background-color: rgba(0, 0, 0, 0.2);",
            onclick: move |_| props.on_close.call(()),

            div {
                class: "bg-white p-8 rounded-lg shadow-xl max-w-md w-full m-4 relative text-gray-800",
                onclick: move |evt| evt.stop_propagation(),

                button {
                    class: "absolute top-4 right-4 text-gray-400 hover:text-gray-600",
                    onclick: move |_| props.on_close.call(()),
                    svg {
                        class: "w-6 h-6",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M6 18L18 6M6 6l12 12" }
                    }
                }

                h2 { class: "text-2xl font-bold mb-2 text-center", "Proxy Nexus" }
                p { class: "text-center mb-8", "by Alex McCulloch (axmccx)" }

                h5 { class: "text-sm font-bold mb-2 text-center", "with contributions from:" }
                p { class: "text-center mb-8", "Fatih Inan" }

                div { class: "flex flex-col gap-4 text-blue-600 items-center font-medium",
                    a { href: instructions_href, target: "_blank", class: "hover:underline", "Instructions"}
                    a { href: "https://github.com/axmccx/proxynexus-rs", target: "_blank", class: "hover:underline", "GitHub" }
                    a { href: "https://us.posthog.com/shared/Mo4ZScqPqTkiJ01AEJtx8pzc4YqBag", target: "_blank", class: "hover:underline", "Statistics" }
                    a { href: "https://ko-fi.com/axmccx", target: "_blank", class: "hover:underline", "Donate" }
                }
            }
        }
    }
}

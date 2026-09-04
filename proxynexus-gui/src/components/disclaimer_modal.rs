use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct DisclaimerModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn DisclaimerModal(props: DisclaimerModalProps) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 flex items-center justify-center z-[3000]",
            style: "background-color: rgba(0, 0, 0, 0.2);",
            onclick: move |_| props.on_close.call(()),

            div {
                class: "bg-white p-8 rounded-lg shadow-xl max-w-lg w-full m-4 relative text-gray-800 max-h-[90vh] overflow-y-auto",
                onclick: move |evt| evt.stop_propagation(),

                h2 { class: "text-2xl font-bold mb-4 text-center", "Disclaimer" }

                p { class: "text-sm mb-3 leading-relaxed",
                    "Proxy Nexus is an unofficial, non-commercial fan project dedicated to helping players preserve and print proxy cards for out-of-print and legacy card games."
                }
                p { class: "text-sm mb-3 leading-relaxed",
                    "Proxy Nexus is not affiliated with, endorsed by, or sponsored by Fantasy Flight Games, Asmodee, or any other publisher or rights holder."
                }
                p { class: "text-sm mb-3 leading-relaxed",
                    "Proxy Nexus is limited to legacy content and products that are no longer commercially available and have no known future print runs scheduled."
                }
                p { class: "text-sm mb-3 leading-relaxed",
                    "All game names, artwork, logos, trademarks, and other intellectual property are the property of their respective owners."
                }
                p { class: "text-sm mb-6 leading-relaxed",
                    "By continuing, you acknowledge that Proxy Nexus is an independent fan project and that it is not an official source for any of the games or content available through the site."
                }

                div { class: "flex justify-center",
                    button {
                        class: "px-6 py-2 bg-blue-600 text-white font-medium rounded-md hover:bg-blue-700 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400",
                        onclick: move |_| props.on_close.call(()),
                        "OK"
                    }
                }
            }
        }
    }
}

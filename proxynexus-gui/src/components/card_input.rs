use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct CardInputProps {
    pub text_state: Signal<String>,
}

#[component]
pub fn CardInput(mut props: CardInputProps) -> Element {
    rsx! {
        div {
            class: "flex flex-col flex-1 p-4 w-full",
            h2 { class: "text-lg font-bold mb-4 text-gray-800", "Sources" }
            textarea {
                class: "flex-1 w-full p-3 border border-gray-300 rounded-md shadow-sm outline-none focus:ring-2 focus:ring-blue-400 resize-none font-mono text-sm",
                placeholder: "Enter your card list here (e.g. 3x Sure Gamble)...",
                value: "{props.text_state}",
                oninput: move |evt| {
                    props.text_state.set(evt.value());
                }
            }
        }
    }
}

use dioxus::prelude::*;
use proxynexus_core::pdf::PageSize;

#[derive(Props, Clone, PartialEq)]
pub struct ExportControlsProps {
    pub on_generate: EventHandler<PageSize>,
}

#[component]
pub fn ExportControls(props: ExportControlsProps) -> Element {
    let mut page_size = use_signal(PageSize::default);

    rsx! {
        div {
            class: "p-4 border-t border-gray-200 bg-gray-50 flex flex-col gap-4",
            h2 { class: "text-lg font-bold text-gray-800", "Export" }

            div { class: "flex flex-col gap-2",
                label { class: "text-sm font-medium text-gray-700", "Page Size" }
                select {
                    class: "w-full p-2 border border-gray-300 rounded-md outline-none focus:ring-2 focus:ring-blue-400 bg-white text-sm",
                    value: match page_size() {
                        PageSize::A4 => "A4",
                        PageSize::Letter => "Letter",
                    },
                    onchange: move |evt| {
                        let selected = match evt.value().as_str() {
                            "A4" => PageSize::A4,
                            _ => PageSize::Letter,
                        };
                        page_size.set(selected);
                    },
                    option { value: "Letter", "Letter" }
                    option { value: "A4", "A4" }
                }
            }

            button {
                class: "w-full py-2 px-4 bg-blue-600 hover:bg-blue-700 text-white font-semibold rounded-md shadow-sm transition-colors mt-2",
                onclick: move |_| props.on_generate.call(page_size()),
                "Generate PDF"
            }
        }
    }
}

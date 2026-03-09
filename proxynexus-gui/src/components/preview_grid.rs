use dioxus::prelude::*;
use proxynexus_core::models::Printing;

#[cfg(target_arch = "wasm32")]
const R2_BASE_URL: &str = "https://collections.proxynexus.net";

fn build_image_url(image_key: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        format!("proxynexus://images/{}", image_key)
    }

    #[cfg(feature = "web")]
    {
        format!("{}/{}", R2_BASE_URL, image_key)
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PreviewGridProps {
    pub printings: Vec<Printing>,
}

#[component]
pub fn PreviewGrid(props: PreviewGridProps) -> Element {
    rsx! {
        div {
            class: "flex flex-wrap gap-4",
            for (index, printing) in props.printings.iter().enumerate() {
                div {
                    key: "{index}",
                    class: "w-[200px] overflow-hidden shadow-lg aspect-[2.5/3.5] bg-gray-400",
                    img {
                        src: "{build_image_url(&printing.image_key)}",
                        class: "w-full h-full",
                        alt: "{printing.card_title}",
                    }
                }
            }
        }
    }
}

pub mod about_modal;
pub mod card_list_input;
pub mod export_controls;
pub mod preview_grid;
pub mod print_layout_info;
pub mod source_selector;
pub mod upscale_info;
pub mod variant_selector;

pub(crate) fn build_image_url(image_key: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        format!("proxynexus://collections/{}", image_key)
    }

    #[cfg(feature = "web")]
    {
        format!("https://test-collections.proxynexus.net/{}", image_key)
    }
}

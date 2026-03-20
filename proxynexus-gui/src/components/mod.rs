pub mod export_controls;
pub mod preview_grid;
pub mod source_selector;
pub mod variant_selector;

pub(crate) fn build_image_url(image_key: &str) -> String {
    #[cfg(feature = "desktop")]
    {
        format!("proxynexus://collections/{}", image_key)
    }

    #[cfg(feature = "web")]
    {
        format!("https://collections.proxynexus.net/{}", image_key)
    }
}

pub mod app;

#[cfg(feature = "csr")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use crate::app::CommentSystem;
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;
    
    console_error_panic_hook::set_once();

    if let Some(el) = document().get_element_by_id("tinydis") {
        let page_id = el
            .get_attribute("data-page-id")
            .unwrap_or_else(|| "".into());

        let server_url = el
            .get_attribute("data-server-url")
            .expect("No data-server-url set!");
        let static_url: &'static str = Box::leak(server_url.into_boxed_str());
        leptos::prelude::server_fn::client::set_server_url(static_url);

        let html_el = el.unchecked_into();
        let handle =
            leptos::mount::mount_to(html_el, move || view! { <CommentSystem page_id=page_id /> });
        handle.forget();
    }
}

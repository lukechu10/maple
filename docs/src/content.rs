use maple_core::prelude::*;
use pulldown_cmark::{html, Options, Parser};
use web_sys::HtmlElement;

pub fn Content<G: GenericNode>() -> TemplateResult<G> {
    let location = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .location()
        .unwrap();
    let pathname = location.pathname().unwrap();

    let docs_container_ref = NodeRef::<G>::new();

    let markdown = Signal::new(String::new());
    let html = create_memo(cloned!((markdown) => move || {
        let markdown = markdown.get();

        let options = Options::empty();
        let parser = Parser::new_ext(markdown.as_ref(), options);

        let mut output = String::new();
        html::push_html(&mut output, parser);

        output
    }));

    create_effect(cloned!((docs_container_ref) => move || {
        if !html.get().is_empty() {
            docs_container_ref.get::<DomNode>().unchecked_into::<HtmlElement>().set_inner_html(html.get().as_ref());
        }
    }));

    wasm_bindgen_futures::spawn_local(cloned!((markdown) => async move {
        log::info!("Getting documentation at {}", pathname);

        let url = format!("{}/markdown{}.md", location.origin().unwrap(), pathname);
        match reqwest::get(url).await {
            Ok(res) => {
                markdown.set(res.text().await.unwrap());
            }
            Err(err) => {
                log::error!("Unknown error: {}", err);
            }
        }
    }));

    template! {
        div(ref=docs_container_ref, id="docs-container") { "Loading..." }
    }
}

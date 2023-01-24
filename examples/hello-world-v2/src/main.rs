use sycamore_core2::view::ToView;
use sycamore_reactive::*;
use sycamore_web2::html::elements::*;
use sycamore_web2::html::*;
use sycamore_web2::*;
use wasm_bindgen::JsCast;

fn app(cx: Scope) -> View {
    let mut counter = create_signal(cx, 0);
    let increment = move |_| counter += 1;
    let decrement = move |_| counter -= 1;
    let reset = move |_| counter.set(0);
    div(cx)
        .class("container")
        .child(
            p(cx)
                .dynamic(move |e| e.class(format!("counter-{counter}")))
                .child("Counter value: ")
                .child(View::new_dyn(cx, move || counter.get().to_view(cx))),
        )
        .child(button(cx).child("+").on(on::click, increment))
        .child(button(cx).child("-").on(on::click, decrement))
        .child(button(cx).child("Reset").on(on::click, reset))
        .view()
}

fn main() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());
    let ssr = sycamore_web2::render::render_to_string(app);
    log::info!("{ssr}");

    let root = document().get_element_by_id("main").unwrap();

    // root.set_inner_html(&ssr);
    // sycamore_web2::render::hydrate_to(root.unchecked_into(), app);

    sycamore_web2::render::render_to(root.unchecked_into(), app);
}

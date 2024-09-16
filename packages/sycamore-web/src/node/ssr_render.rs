use super::*;

/// The mode in which SSR is being run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsrMode {
    /// Synchronous mode.
    ///
    /// When a suspense boundary is hit, only the fallback is rendered.
    Sync,
    /// Blocking mode.
    ///
    /// When a suspense boundary is hit, rendering is paused until the suspense is resolved.
    Blocking,
    /// Streaming mode.
    ///
    /// When a suspense boundary is hit, the fallback is rendered. Once the suspense is resolved,
    /// the rendered HTML is streamed to the client.
    Streaming,
}

/// Render a [`View`] into a static [`String`]. Useful for rendering to a string on the server side.
#[must_use]
pub fn render_to_string(view: impl FnOnce() -> View) -> String {
    is_not_ssr! {
        let _ = view;
        panic!("`render_to_string` only available in SSR mode");
    }
    is_ssr! {
        use std::cell::LazyCell;

        thread_local! {
            /// Use a static variable here so that we can reuse the same root for multiple calls to
            /// this function.
            static SSR_ROOT: LazyCell<RootHandle> = LazyCell::new(|| create_root(|| {}));
        }
        let mut buf = String::new();
        SSR_ROOT.with(|root| {
            root.dispose();
            root.run_in(|| {
                // We run this in a new scope so that we can dispose everything after we render it.
                provide_context(HydrationRegistry::new());
                provide_context(SsrMode::Sync);

                IS_HYDRATING.set(true);
                let view = view();
                IS_HYDRATING.set(false);
                ssr_node::render_recursive_view(&view, &mut buf);
            });
        });
        buf
    }
}

/// Renders a [`View`] into a static [`String`] while awaiting for all suspense boundaries to
/// resolve. Useful for rendering to a string on the server side.
#[must_use]
#[cfg(feature = "suspense")]
pub async fn render_to_string_await_suspense(view: impl FnOnce() -> View) -> String {
    is_not_ssr! {
        let _ = view;
        panic!("`render_to_string` only available in SSR mode");
    }
    is_ssr! {
        use std::cell::LazyCell;
        use std::fmt::Write;
        use std::collections::HashMap;

        use futures::StreamExt;

        const BUFFER_SIZE: usize = 5;

        thread_local! {
            /// Use a static variable here so that we can reuse the same root for multiple calls to
            /// this function.
            static SSR_ROOT: LazyCell<RootHandle> = LazyCell::new(|| create_root(|| {}));
        }
        IS_HYDRATING.set(true);
        sycamore_futures::provide_executor_scope(async {
            let mut buf = String::new();

            let (sender, mut receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
            SSR_ROOT.with(|root| {
                root.dispose();
                root.run_in(|| {
                    // We run this in a new scope so that we can dispose everything after we render it.
                    provide_context(HydrationRegistry::new());
                    provide_context(SsrMode::Blocking);
                    let suspense_state = SuspenseState { sender };

                    provide_context(suspense_state);

                    let view = view();
                    ssr_node::render_recursive_view(&view, &mut buf);
                });
            });

            // Split at suspense fragment locations.
            let split = buf.split("<!--sycamore-suspense-").collect::<Vec<_>>();
            // Calculate the number of suspense fragments.
            let n = split.len() - 1;

            // Now we wait until all suspense fragments are resolved.
            let mut fragment_map = HashMap::new();
            if n == 0 {
                receiver.close();
            }
            let mut i = 0;
            while let Some(fragment) = receiver.next().await {
                fragment_map.insert(fragment.key, fragment.view);
                i += 1;
                if i == n {
                    // We have received all suspense fragments so we shouldn't need the receiver anymore.
                    receiver.close();
                }
            }
            IS_HYDRATING.set(false);

            // Finally, replace all suspense marker nodes with rendered values.
            if let [first, rest @ ..] = split.as_slice() {
                rest.iter().fold(first.to_string(), |mut acc, s| {
                    // Try to parse the key.
                    let (num, rest) = s.split_once("-->").expect("end of suspense marker not found");
                    let key: u32 = num.parse().expect("could not parse suspense key");
                    let fragment = fragment_map.get(&key).expect("fragment not found");
                    ssr_node::render_recursive_view(fragment, &mut acc);

                    write!(&mut acc, "{rest}").unwrap();
                    acc
                })
            } else {
                unreachable!("split should always have at least one element")
            }
        }).await
    }
}

/// Renders a [`View`] to a stream.
///
/// TODO: write docs
#[cfg(feature = "suspense")]
pub fn render_to_string_stream(
    view: impl FnOnce() -> View,
) -> impl futures::Stream<Item = String> + Send {
    is_not_ssr! {
        let _ = view;
        panic!("`render_to_string` only available in SSR mode");
    }
    is_ssr! {
        use std::cell::LazyCell;

        use futures::StreamExt;

        const BUFFER_SIZE: usize = 5;

        thread_local! {
            /// Use a static variable here so that we can reuse the same root for multiple calls to
            /// this function.
            static SSR_ROOT: LazyCell<RootHandle> = LazyCell::new(|| create_root(|| {}));
        }
        IS_HYDRATING.set(true);
        let mut buf = String::new();
        let (sender, mut receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
        SSR_ROOT.with(|root| {
            root.dispose();
            root.run_in(|| {
                // We run this in a new scope so that we can dispose everything after we render it.
                provide_context(HydrationRegistry::new());
                provide_context(SsrMode::Streaming);
                let suspense_state = SuspenseState { sender };

                provide_context(suspense_state);

                let view = view();
                ssr_node::render_recursive_view(&view, &mut buf);
            });
        });

        // Calculate the number of suspense fragments.
        let mut n = buf.matches("<!--sycamore-suspense-").count();

        // ```js
        // function __sycamore_suspense(key) {
        //   let start = document.querySelector(`suspense-start[data-key="${key}"]`)
        //   let end = document.querySelector(`suspense-end[data-key="${key}"]`)
        //   let template = document.getElementById(`sycamore-suspense-${key}`)
        //   start.parentNode.insertBefore(template.content, start)
        //   while (start.nextSibling != end) {
        //     start.parentNode.removeChild(start.nextSibling)
        //   }
        //   start.remove()
        //   end.remove()
        // }
        // ```
        static SUSPENSE_REPLACE_SCRIPT: &str = r#"<script>function __sycamore_suspense(e){let s=document.querySelector(`suspense-start[data-key="${e}"]`),n=document.querySelector(`suspense-end[data-key="${e}"]`),r=document.getElementById(`sycamore-suspense-${e}`);for(s.parentNode.insertBefore(r.content,s);s.nextSibling!=n;)s.parentNode.removeChild(s.nextSibling);s.remove(),n.remove()}</script>"#;
        async_stream::stream! {
            let mut initial = String::new();
            initial.push_str("<!doctype html>");
            initial.push_str(&buf);
            initial.push_str(SUSPENSE_REPLACE_SCRIPT);
            yield initial;

            if n == 0 {
                receiver.close();
            }
            let mut i = 0;
            while let Some(fragment) = receiver.next().await {
                let buf_fragment = render_suspense_fragment(fragment);
                // Check if we have any nested suspense.
                let n_add = buf_fragment.matches("<!--sycamore-suspense-").count();
                n += n_add;

                yield buf_fragment;

                i += 1;
                if i == n {
                    // We have received all suspense fragments so we shouldn't need the receiver anymore.
                    receiver.close();
                }
            }
        }
    }
}

#[cfg_ssr]
#[cfg(feature = "suspense")]
fn render_suspense_fragment(SuspenseFragment { key, view }: SuspenseFragment) -> String {
    use std::fmt::Write;

    let mut buf = String::new();
    write!(&mut buf, "<template id=\"sycamore-suspense-{key}\">",).unwrap();
    ssr_node::render_recursive_view(&view, &mut buf);
    write!(
        &mut buf,
        "</template><script>__sycamore_suspense({key})</script>"
    )
    .unwrap();

    buf
}

#[cfg(test)]
#[cfg(feature = "suspense")]
mod tests {
    use futures::channel::oneshot;

    use super::*;

    #[component(inline_props)]
    async fn AsyncComponent(receiver: oneshot::Receiver<()>) -> View {
        receiver.await.unwrap();
        view! {
            "Hello, async!"
        }
    }

    #[component(inline_props)]
    fn App(receiver: oneshot::Receiver<()>) -> View {
        view! {
            Suspense(fallback="fallback".into()) {
                AsyncComponent(receiver=receiver)
            }
        }
    }

    #[test]
    fn render_to_string_renders_fallback() {
        let (sender, receiver) = oneshot::channel();
        let res = render_to_string(move || view! { App(receiver=receiver) });
        assert_eq!(res, "<!--/-->fallback<!--/-->");
        assert!(sender.send(()).is_err(), "receiver should be dropped");
    }

    #[tokio::test]
    async fn render_to_string_await_suspense_works() {
        let (sender, receiver) = oneshot::channel();
        let ssr = render_to_string_await_suspense(move || view! { App(receiver=receiver) });
        futures::pin_mut!(ssr);
        assert!(futures::poll!(&mut ssr).is_pending());

        sender.send(()).unwrap();
        let res = ssr.await;
        assert_eq!(res, "<!--/--><!--/-->Hello, async!<!--/--><!--/-->");
    }
}

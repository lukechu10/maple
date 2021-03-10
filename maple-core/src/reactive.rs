//! Reactive primitives.

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

/// State of the current running effect.
struct Running {
    execute: Callback,
    dependencies: Vec<Rc<dyn AnySignalInner>>,
}

thread_local! {
    /// Context of the effect that is currently running. `None` if no effect is running.
    ///
    /// This is an array of callbacks that, when called, will add the a `Signal` to the `handle` in the argument.
    /// The callbacks return another callback which will unsubscribe the `handle` from the `Signal`.
    static CONTEXTS: RefCell<Vec<Rc<RefCell<Option<Running>>>>> = RefCell::new(Vec::new());
}

#[derive(Clone)]
struct Callback(Rc<dyn Fn()>);

/// Returned by functions that provide a handle to access state.
pub struct StateHandle<T: 'static>(Rc<RefCell<SignalInner<T>>>);

impl<T: 'static> StateHandle<T> {
    /// Get the current value of the state.
    pub fn get(&self) -> Rc<T> {
        // if inside an effect, add this signal to dependency list
        CONTEXTS.with(|contexts| {
            if !contexts.borrow().is_empty() {
                let signal = self.0.clone();

                if contexts
                    .borrow()
                    .last()
                    .unwrap()
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .dependencies
                    .iter()
                    .find(|dependency| {
                        dependency.as_ref() as *const _ == signal.as_ref() as *const _
                        /* do reference equality */
                    })
                    .is_none()
                {
                    contexts
                        .borrow()
                        .last()
                        .unwrap()
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .dependencies
                        .push(signal);
                }
            }
        });

        self.get_untracked()
    }

    /// Get the current value of the state, without tracking this as a dependency if inside a
    /// reactive context.
    ///
    /// # Example
    ///
    /// ```
    /// use maple_core::prelude::*;
    ///
    /// let state = Signal::new(1);
    ///
    /// let double = create_memo({
    ///     let state = state.clone();
    ///     move || *state.get_untracked() * 2
    /// });
    ///
    /// assert_eq!(*double.get(), 2);
    ///
    /// state.set(2);
    /// // double value should still be old value because state was untracked
    /// assert_eq!(*double.get(), 2);
    /// ```
    pub fn get_untracked(&self) -> Rc<T> {
        self.0.borrow().inner.clone()
    }
}

impl<T: 'static> Clone for StateHandle<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// State that can be set.
pub struct Signal<T: 'static> {
    handle: StateHandle<T>,
}

impl<T: 'static> Signal<T> {
    /// Creates a new signal.
    ///
    /// # Examples
    ///
    /// ```
    /// use maple_core::prelude::*;
    ///
    /// let state = Signal::new(0);
    /// assert_eq!(*state.get(), 0);
    ///
    /// state.set(1);
    /// assert_eq!(*state.get(), 1);
    /// ```
    pub fn new(value: T) -> Self {
        Self {
            handle: StateHandle(Rc::new(RefCell::new(SignalInner::new(value)))),
        }
    }

    /// Set the current value of the state.
    ///
    /// This will notify and update any effects and memos that depend on this value.
    pub fn set(&self, new_value: T) {
        match self.handle.0.try_borrow_mut() {
            Ok(mut signal) => signal.update(new_value),
            // If the signal is already borrowed, that means it is borrowed in the getter, thus creating a cyclic dependency.
            Err(_err) => panic!("cannot create cyclic dependency"),
        }

        // Clone subscribers to prevent modifying list when calling callbacks.
        let subscribers = self.handle.0.borrow().subscribers.clone();

        for subscriber in subscribers {
            subscriber.0();
        }
    }

    /// Get the [`StateHandle`] associated with this signal.
    ///
    /// This is a shortcut for `(*signal).clone()`.
    pub fn handle(&self) -> StateHandle<T> {
        self.handle.clone()
    }

    /// Convert this signal into its underlying handle.
    pub fn into_handle(self) -> StateHandle<T> {
        self.handle
    }
}

impl<T: 'static> Deref for Signal<T> {
    type Target = StateHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
        }
    }
}

struct SignalInner<T> {
    inner: Rc<T>,
    subscribers: Vec<Callback>,
}

impl<T> SignalInner<T> {
    fn new(value: T) -> Self {
        Self {
            inner: Rc::new(value),
            subscribers: Vec::new(),
        }
    }

    /// Adds a handler to the subscriber list. If the handler is already a subscriber, does nothing.
    fn subscribe(&mut self, handler: Callback) {
        // make sure handler is not already in self.observers
        if self
            .subscribers
            .iter()
            .find(|subscriber| {
                subscriber.0.as_ref() as *const _ == handler.0.as_ref() as *const _
                /* do reference equality */
            })
            .is_none()
        {
            self.subscribers.push(handler);
        }
    }

    /// Removes a handler from the subscriber list. If the handler is not a subscriber, does nothing.
    fn unsubscribe(&mut self, handler: &Callback) {
        self.subscribers = self
            .subscribers
            .iter()
            .filter(|subscriber| {
                if subscriber.0.as_ref() as *const _ == handler.0.as_ref() as *const _ {
                    eprintln!("unsubscribed {:?}", subscriber.0.as_ref() as *const _);
                }
                subscriber.0.as_ref() as *const _ == handler.0.as_ref() as *const _
                /* do reference equality */
            })
            .cloned()
            .collect();
    }

    /// Updates the inner value. This does **NOT** call the subscribers.
    /// You will have to do so manually with `trigger_subscribers`.
    fn update(&mut self, new_value: T) {
        self.inner = Rc::new(new_value);
    }
}

/// Trait for any [`SignalInner`], regardless of type param `T`.
trait AnySignalInner {
    fn subscribe(&self, handler: Callback);
    fn unsubscribe(&self, handler: &Callback);
}

impl<T> AnySignalInner for RefCell<SignalInner<T>> {
    fn subscribe(&self, handler: Callback) {
        self.borrow_mut().subscribe(handler);
    }

    fn unsubscribe(&self, handler: &Callback) {
        self.borrow_mut().unsubscribe(handler);
    }
}

fn cleanup_running(running: &Rc<RefCell<Option<Running>>>) {
    let execute = running.borrow().as_ref().unwrap().execute.clone();

    for dependency in &running.borrow().as_ref().unwrap().dependencies {
        eprintln!(
            "trying to unsubscribe {:?}",
            dependency.as_ref() as *const _
        );
        dependency.unsubscribe(&execute);
    }

    running.borrow_mut().as_mut().unwrap().dependencies.clear();
}

/// Creates an effect on signals used inside the effect closure.
///
/// Unlike [`create_effect`], this will allow the closure to run different code upon first
/// execution, so it can return a value.
fn create_effect_initial<R>(initial: impl Fn() -> (Rc<Callback>, R) + 'static) -> R {
    CONTEXTS.with(|contexts| {
        let running = Running {
            execute: Callback(Rc::new(|| {})),
            dependencies: Vec::new(),
        };

        contexts
            .borrow_mut()
            .push(Rc::new(RefCell::new(Some(running))));

        // run effect for the first time to attach all the dependencies
        let (effect, ret) = initial();

        let subscribe_callback = Callback(Rc::new(move || {
            effect.0();
        }));

        // attach dependencies
        for dependency in &contexts
            .borrow()
            .last()
            .unwrap()
            .borrow()
            .as_ref()
            .unwrap()
            .dependencies
        {
            dependency.subscribe(subscribe_callback.clone());
        }

        // Reset dependencies for next effect hook
        contexts.borrow_mut().pop().unwrap();

        ret
    })
}

/// Creates an effect on signals used inside the effect closure.
pub fn create_effect<F>(effect: F)
where
    F: Fn() + 'static,
{
    let running = Rc::new(RefCell::new(None));

    let execute = Callback(Rc::new({
        let running = running.clone();
        move || {
            CONTEXTS.with(|contexts| {
                let initial_context_size = contexts.borrow().len();

                cleanup_running(&running);
                debug_assert!(running.borrow().as_ref().unwrap().dependencies.is_empty());

                contexts.borrow_mut().push(running.clone());

                effect();

                // attach dependencies
                for dependency in &contexts
                    .borrow()
                    .last()
                    .unwrap()
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .dependencies
                {
                    eprintln!("subscribed to {:?}", dependency.as_ref() as *const _);
                    dependency.subscribe(running.borrow().as_ref().unwrap().execute.clone());
                }

                contexts.borrow_mut().pop();

                debug_assert_eq!(
                    initial_context_size,
                    contexts.borrow().len(),
                    "context size should not change"
                );
            });
        }
    }));

    *running.borrow_mut() = Some(Running {
        execute: execute.clone(),
        dependencies: Vec::new(),
    });

    execute.0()
}

/// Creates a memoized value from some signals. Also know as "derived stores".
pub fn create_memo<F, Out>(derived: F) -> StateHandle<Out>
where
    F: Fn() -> Out + 'static,
    Out: 'static,
{
    create_selector_with(derived, |_, _| false)
}

/// Creates a memoized value from some signals. Also know as "derived stores".
/// Unlike [`create_memo`], this function will not notify dependents of a change if the output is the same.
/// That is why the output of the function must implement [`PartialEq`].
///
/// To specify a custom comparison function, use [`create_selector_with`].
pub fn create_selector<F, Out>(derived: F) -> StateHandle<Out>
where
    F: Fn() -> Out + 'static,
    Out: PartialEq + 'static,
{
    create_selector_with(derived, PartialEq::eq)
}

/// Creates a memoized value from some signals. Also know as "derived stores".
/// Unlike [`create_memo`], this function will not notify dependents of a change if the output is the same.
///
/// It takes a comparison function to compare the old and new value, which returns `true` if they
/// are the same and `false` otherwise.
///
/// To use the type's [`PartialEq`] implementation instead of a custom function, use
/// [`create_selector`].
pub fn create_selector_with<F, Out, C>(derived: F, comparator: C) -> StateHandle<Out>
where
    F: Fn() -> Out + 'static,
    Out: 'static,
    C: Fn(&Out, &Out) -> bool + 'static,
{
    let derived = Rc::new(derived);
    let comparator = Rc::new(comparator);

    create_effect_initial(move || {
        let memo = Signal::new(derived());

        let effect = Rc::new(Callback(Rc::new({
            let memo = memo.clone();
            let derived = derived.clone();
            let comparator = comparator.clone();
            move || {
                let new_value = derived();
                if !comparator(&memo.get_untracked(), &new_value) {
                    memo.set(new_value);
                }
            }
        })));

        (effect, memo.into_handle())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals() {
        let state = Signal::new(0);
        assert_eq!(*state.get(), 0);

        state.set(1);
        assert_eq!(*state.get(), 1);
    }

    #[test]
    fn signal_composition() {
        let state = Signal::new(0);

        let double = || *state.get() * 2;

        assert_eq!(double(), 0);

        state.set(1);
        assert_eq!(double(), 2);
    }

    #[test]
    fn effects() {
        let state = Signal::new(0);

        let double = Signal::new(-1);

        create_effect({
            let state = state.clone();
            let double = double.clone();
            move || {
                double.set(*state.get() * 2);
            }
        });
        assert_eq!(*double.get(), 0); // calling create_effect should call the effect at least once

        state.set(1);
        assert_eq!(*double.get(), 2);
        state.set(2);
        assert_eq!(*double.get(), 4);
    }

    #[test]
    #[ignore]
    #[should_panic(expected = "cannot create cyclic dependency")]
    fn cyclic_effects_fail() {
        let state = Signal::new(0);

        create_effect({
            let state = state.clone();
            move || {
                state.set(*state.get() + 1);
            }
        });

        state.set(1);
    }

    #[test]
    #[ignore]
    #[should_panic(expected = "cannot create cyclic dependency")]
    fn cyclic_effects_fail_2() {
        let state = Signal::new(0);

        create_effect({
            let state = state.clone();
            move || {
                let value = *state.get();
                state.set(value + 1);
            }
        });

        state.set(1);
    }

    #[test]
    fn effect_should_subscribe_once() {
        let state = Signal::new(0);

        let counter = Signal::new(0);
        create_effect({
            let state = state.clone();
            let counter = counter.clone();
            move || {
                counter.set(*counter.get_untracked() + 1);

                // call state.get() twice but should subscribe once
                state.get();
                state.get();
            }
        });

        assert_eq!(*counter.get(), 1);

        state.set(1);
        assert_eq!(*counter.get(), 2);
    }

    #[test]
    fn effect_should_recreate_dependencies() {
        let condition = Signal::new(true);

        let state1 = Signal::new(0);
        let state2 = Signal::new(1);

        let counter = Signal::new(0);
        create_effect({
            let condition = condition.clone();
            let state1 = state1.clone();
            let state2 = state2.clone();
            let counter = counter.clone();

            eprintln!("condition: {:?}", condition.handle.0.as_ref() as *const _);
            eprintln!("state1: {:?}", state1.handle.0.as_ref() as *const _);
            eprintln!("state2: {:?}", state2.handle.0.as_ref() as *const _);

            move || {
                counter.set(*counter.get_untracked() + 1);

                if *condition.get() {
                    state1.get();
                } else {
                    state2.get();
                }
            }
        });

        assert_eq!(*counter.get(), 1);

        state1.set(1);
        assert_eq!(*counter.get(), 2);

        state2.set(1);
        assert_eq!(*counter.get(), 2); // not tracked

        condition.set(false);
        assert_eq!(*counter.get(), 3);

        state1.set(2);
        assert_eq!(*counter.get(), 3); // not tracked

        state2.set(2);
        assert_eq!(*counter.get(), 4); // tracked after condition.set
    }

    #[test]
    fn memo() {
        let state = Signal::new(0);

        let double = create_memo({
            let state = state.clone();
            move || *state.get() * 2
        });
        assert_eq!(*double.get(), 0);

        state.set(1);
        assert_eq!(*double.get(), 2);

        state.set(2);
        assert_eq!(*double.get(), 4);
    }

    #[test]
    /// Make sure value is memoized rather than executed on demand.
    fn memo_only_run_once() {
        let state = Signal::new(0);

        let counter = Signal::new(0);
        let double = create_memo({
            let state = state.clone();
            let counter = counter.clone();
            move || {
                counter.set(*counter.get_untracked() + 1);

                *state.get() * 2
            }
        });
        assert_eq!(*counter.get(), 1); // once for calculating initial derived state

        state.set(2);
        assert_eq!(*counter.get(), 2);
        assert_eq!(*double.get(), 4);
        assert_eq!(*counter.get(), 2); // should still be 2 after access
    }

    #[test]
    fn dependency_on_memo() {
        let state = Signal::new(0);

        let double = create_memo({
            let state = state.clone();
            move || *state.get() * 2
        });

        let quadruple = create_memo(move || *double.get() * 2);

        assert_eq!(*quadruple.get(), 0);

        state.set(1);
        assert_eq!(*quadruple.get(), 4);
    }

    #[test]
    fn untracked_memo() {
        let state = Signal::new(1);

        let double = create_memo({
            let state = state.clone();
            move || *state.get_untracked() * 2
        });

        assert_eq!(*double.get(), 2);

        state.set(2);
        assert_eq!(*double.get(), 2); // double value should still be true because state.get() was inside untracked
    }

    #[test]
    fn selector() {
        let state = Signal::new(0);

        let double = create_selector({
            let state = state.clone();
            move || *state.get() * 2
        });

        let counter = Signal::new(0);
        create_effect({
            let counter = counter.clone();
            let double = double.clone();
            move || {
                counter.set(*counter.get_untracked() + 1);

                double.get();
            }
        });
        assert_eq!(*double.get(), 0);
        assert_eq!(*counter.get(), 1);

        state.set(0);
        assert_eq!(*double.get(), 0);
        assert_eq!(*counter.get(), 1); // calling set_state should not trigger the effect

        state.set(2);
        assert_eq!(*double.get(), 4);
        assert_eq!(*counter.get(), 2);
    }
}

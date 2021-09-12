//! Client-side hydration for Sycamore.

pub struct HydrationRegistry {
    current_id: u32,
}

impl HydrationRegistry {
    /// Create a new [`HydrationRegistry`].
    #[inline]
    pub fn new() -> Self {
        Self { current_id: 1 }
    }

    /// Increments the id and returns a hydration key with the old id.
    #[inline]
    pub fn next_key(&mut self) -> HydrationKey {
        let id = self.current_id;
        self.current_id = id + 1;
        HydrationKey(id)
    }

    /// Gets a hydration key with the current id.
    #[inline]
    pub fn current_key(&mut self) -> HydrationKey {
        HydrationKey(self.current_id)
    }
}

impl Default for HydrationRegistry {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub struct HydrationKey(pub u32);

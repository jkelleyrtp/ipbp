use std::mem::MaybeUninit;

trait DerefHelper {
    /// The target type of the reference.
    type Target: ?Sized + 'static;

    fn read(self: &Self) -> &Self::Target;

    /// SAFETY: You must call this function directly with `self` as the argument.
    /// This function relies on the size of the object you return from the deref
    /// being the same as the object you pass in
    #[doc(hidden)]
    unsafe fn deref_impl<'a>(&self) -> &'a dyn Fn() -> Self::Target
    where
        Self: Sized + 'a,
        Self::Target: Clone,
    {
        // https://github.com/dtolnay/case-studies/tree/master/callable-types

        // First we create a closure that captures something with the Same in memory layout as Self (MaybeUninit<Self>).
        let uninit_callable = MaybeUninit::<Self>::uninit();
        // Then move that value into the closure. We assume that the closure now has a in memory layout of Self.
        let uninit_closure = move || Self::read(unsafe { &*uninit_callable.as_ptr() }).clone();

        // Check that the size of the closure is the same as the size of Self in case the compiler changed the layout of the closure.
        let size_of_closure = std::mem::size_of_val(&uninit_closure);
        assert_eq!(size_of_closure, std::mem::size_of::<Self>());

        // Then cast the lifetime of the closure to the lifetime of &self.
        fn cast_lifetime<'a, T>(_a: &T, b: &'a T) -> &'a T {
            b
        }
        let reference_to_closure = cast_lifetime(
            {
                // The real closure that we will never use.
                &uninit_closure
            },
            #[allow(clippy::missing_transmute_annotations)]
            // We transmute self into a reference to the closure. This is safe because we know that the closure has the same memory layout as Self so &Closure == &Self.
            unsafe {
                std::mem::transmute(self)
            },
        );

        // Cast the closure to a trait object.
        reference_to_closure as &_
    }
}

// impl<T: Clone> Deref for Memo<T>
// where
//     T: PartialEq,
// {
//     type Target = dyn Fn() -> T;

//     fn deref(&self) -> &Self::Target {
//         unsafe { Readable::deref_impl(self) }
//     }
// }

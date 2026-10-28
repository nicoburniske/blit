use blit::internal::bump::{Bump, Offset, Slice};

#[test]
fn preserves_alignment_and_values_across_growth() {
    #[repr(C, align(64))]
    struct Aligned([u64; 8]);

    let mut bump = Bump::default();
    let first = bump.push(17_u8);
    let second = bump.push(23_u8);
    let numbers = bump.extend([31_u64, 47].into_iter());
    let aligned = bump.push(Aligned([59; 8]));
    assert_eq!(second.byte(), 1);
    assert_eq!(numbers.offset.byte(), 8);
    assert_eq!(aligned.byte(), 64);
    assert_eq!(size_of::<Offset<Aligned>>(), 4);
    assert_eq!(size_of::<Slice<Aligned>>(), 8);
    bump.reserve(4096);
    assert_eq!(bump.as_ptr().addr() % 64, 0);
    // safety: the offsets identify initialized values after growth
    unsafe {
        assert_eq!(*bump.get(first), 17);
        assert_eq!(*bump.get(second), 23);
        assert_eq!(bump.slice(numbers), &[31, 47]);
        assert_eq!(bump.get(aligned).0, [59; 8]);
        *bump.get_mut(first) = 19;
        bump.get_mut(aligned).0[0] = 61;
        assert_eq!(*bump.get(first), 19);
        assert_eq!(bump.get(aligned).0[0], 61);
    }
    let capacity = bump.capacity();
    bump.clear();
    assert_eq!(bump.push(61_u64).byte(), 0);
    assert_eq!(bump.capacity(), capacity);
    // safety: an empty slice does not access storage
    assert!(unsafe { bump.slice(Slice::<u64>::EMPTY) }.is_empty());
}

#[path = "../kernel/src/internal/bump.rs"]
mod bump;

fn main() {
    let mut arena = bump::Bump::default();
    let offset = arena.push(42u32);
    // safety: this offset identifies the value just inserted
    let previous = unsafe { arena.get(offset) };
    arena.clear();
    arena.push(99u32);
    println!("{previous}");
}

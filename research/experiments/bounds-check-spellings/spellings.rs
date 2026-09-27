// Spellings of space removal from a buffer. s1 to s10 work in place; s11
// filters into a new vector and copies it back. Each is compiled with
// -C opt-level=2 and 3.

#[no_mangle]
pub fn s1_index(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        let b = buf[i];
        if b != b' ' {
            buf[kept] = b;
            kept += 1;
        }
    }
    kept
}

#[no_mangle]
pub fn s2_assert_in_loop(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        assert!(kept <= i);
        let b = buf[i];
        if b != b' ' {
            buf[kept] = b;
            kept += 1;
        }
    }
    kept
}

#[no_mangle]
pub fn s3_get_mut(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        let b = buf[i];
        if b != b' ' {
            if let Some(slot) = buf.get_mut(kept) {
                *slot = b;
                kept += 1;
            }
        }
    }
    kept
}

#[no_mangle]
pub fn s4_branchless(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        let b = buf[i];
        buf[kept] = b;
        kept += (b != b' ') as usize;
    }
    kept
}

#[no_mangle]
pub fn s5_while(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    let mut i = 0;
    while i < buf.len() {
        let b = buf[i];
        if b != b' ' {
            buf[kept] = b;
            kept += 1;
        }
        i += 1;
    }
    kept
}

#[no_mangle]
pub fn s6_enumerate_copy(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        let b = buf[i];
        if b != b' ' {
            buf.copy_within(i..i + 1, kept);
            kept += 1;
        }
    }
    kept
}

#[no_mangle]
pub fn s7_cells(buf: &mut [u8]) -> usize {
    let cells = std::cell::Cell::from_mut(buf).as_slice_of_cells();
    let mut kept = 0;
    for cell in cells {
        let b = cell.get();
        if b != b' ' {
            cells[kept].set(b);
            kept += 1;
        }
    }
    kept
}

#[no_mangle]
pub fn s8_unsafe_unchecked(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        let b = buf[i];
        if b != b' ' {
            // SAFETY: kept <= i < buf.len().
            unsafe { *buf.get_unchecked_mut(kept) = b };
            kept += 1;
        }
    }
    kept
}

#[no_mangle]
pub fn s9_unsafe_assume(buf: &mut [u8]) -> usize {
    let mut kept = 0;
    for i in 0..buf.len() {
        // SAFETY: kept grows at most once per i.
        unsafe { std::hint::assert_unchecked(kept <= i) };
        let b = buf[i];
        if b != b' ' {
            buf[kept] = b;
            kept += 1;
        }
    }
    kept
}

#[no_mangle]
pub fn s10_vec_retain(v: &mut Vec<u8>) -> usize {
    v.retain(|&b| b != b' ');
    v.len()
}

#[no_mangle]
pub fn s11_filter_copy_back(buf: &mut [u8]) -> usize {
    let kept: Vec<u8> = buf.iter().copied().filter(|&b| b != b' ').collect();
    for (slot, &b) in buf.iter_mut().zip(kept.iter()) {
        *slot = b;
    }
    kept.len()
}

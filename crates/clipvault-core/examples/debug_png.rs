fn main() {
    let ppm = 5669u32;
    let mut phys = [0u8; 9];
    phys[..4].copy_from_slice(&ppm.to_be_bytes());
    phys[4..8].copy_from_slice(&ppm.to_be_bytes());
    phys[8] = 1;
    let png =
        clipvault_core::rebuild_png_with_metadata(1, 1, &[0, 0, 0, 255], Some(phys), None, None)
            .expect("png");
    std::fs::write("/private/tmp/clipvault-rebuilt-144.png", png).expect("write");
}

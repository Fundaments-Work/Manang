#![no_main]

//! `paginate` and `ManifestAssembler::accept`: valid manifests round-trip in
//! any page order, and hostile pages never panic or yield an unverified list.

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use manang_live::manifest::{
    MAX_MANIFEST_PAGES, MAX_PAGE_ENTRIES, ManifestAssembler, ManifestEntry, manifest_root,
    paginate,
};
use manang_live::project::{FileId, NodeKind};

fn entry(u: &mut Unstructured<'_>, i: usize) -> ManifestEntry {
    let kind = match u.int_in_range(0..=2u8).unwrap_or(0) {
        0 => NodeKind::Dir,
        1 => NodeKind::Text,
        _ => NodeKind::Blob,
    };
    // Mostly valid, distinct paths so the roundtrip reaches multi-page
    // manifests; sometimes a fuzzer-chosen string to exercise validation.
    let path = if u.ratio(1, 8).unwrap_or(false) {
        u.arbitrary::<String>().unwrap_or_default()
    } else {
        format!("d{}/f{i}.txt", i % 7)
    };
    ManifestEntry {
        id: FileId(u.arbitrary().unwrap_or(0)),
        path,
        kind,
        size: if kind == NodeKind::Dir {
            0
        } else {
            u.int_in_range(0..=1 << 20).unwrap_or(0)
        },
        sha256: u.arbitrary().unwrap_or([0; 32]),
        seed_fp: u.arbitrary().unwrap_or([0; 16]),
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let want = u.int_in_range(0..=700usize).unwrap_or(0);
    let entries: Vec<ManifestEntry> = (0..want).map(|i| entry(&mut u, i)).collect();
    // Entries the assembler would refuse make the roundtrip meaningless.
    let hostile = entries.iter().any(|e| e.validate().is_err());

    let pages = paginate(&entries);
    assert!(!pages.is_empty());
    assert!(pages.iter().all(|p| p.len() <= MAX_PAGE_ENTRIES));
    assert_eq!(
        pages.iter().flatten().cloned().collect::<Vec<_>>(),
        entries,
        "paginate loses or reorders entries"
    );
    let root = manifest_root(&entries);
    let total = u32::try_from(pages.len()).expect("page count fits");
    assert!(total <= MAX_MANIFEST_PAGES);

    // Feed the pages in a fuzzer-chosen order, with repeats.
    let mut order: Vec<usize> = (0..pages.len()).collect();
    for i in (1..order.len()).rev() {
        let j = u.int_in_range(0..=i).unwrap_or(i);
        order.swap(i, j);
    }
    let mut asm = ManifestAssembler::new(root, total).expect("page count is in range");
    let mut done = None;
    for (n, &i) in order.iter().enumerate() {
        let r = asm.accept(root, i as u32, total, pages[i].clone());
        if hostile {
            // Any outcome but a panic; a full list must still verify.
            if let Ok(Some(all)) = r {
                assert_eq!(manifest_root(&all), root);
            }
            continue;
        }
        let r = r.expect("a valid page is accepted");
        if n + 1 < order.len() {
            assert!(r.is_none(), "complete before the last page");
        } else {
            done = r;
        }
    }
    if !hostile {
        // The assembler returns pages in index order, which is `entries`.
        assert_eq!(done, Some(entries.clone()));
    }

    // Fully hostile accept: arbitrary header, page and entries.
    let r: [u8; 16] = u.arbitrary().unwrap_or([0; 16]);
    let pages_n: u32 = u.arbitrary().unwrap_or(0);
    if let Ok(mut asm) = ManifestAssembler::new(r, pages_n) {
        for _ in 0..u.int_in_range(0..=4u8).unwrap_or(0) {
            let page: u32 = u.arbitrary().unwrap_or(0);
            let claimed: u32 = if u.ratio(3, 4).unwrap_or(true) {
                pages_n
            } else {
                u.arbitrary().unwrap_or(0)
            };
            let n = u.int_in_range(0..=3usize).unwrap_or(0);
            let es: Vec<ManifestEntry> = (0..n).map(|i| entry(&mut u, i)).collect();
            if let Ok(Some(all)) = asm.accept(r, page, claimed, es) {
                assert_eq!(manifest_root(&all), r);
            }
        }
    }
});

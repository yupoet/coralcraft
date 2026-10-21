use coralcraft_fuzz::run_fuzz_target;

fn main() {
    let seeds: &[&[u8]] = &[
        b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents [4 0 R] >>\nendobj\n4 0 obj\n<< /Length 98 >>\nstream\nq 1 0 0 rg 10 10 50 30 re f BT /F1 12 Tf 1 0 0 1 20 50 Tm [(Hi) -250 <21>] TJ ET Q\nendstream\nendobj\n%%EOF\n",
    ];
    run_fuzz_target(seeds, |data| {
        let _ = coralcraft_doc::import_pdf(data);
    });
}

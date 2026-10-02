# Test fixtures

Personal receipt photos used by the live OCR test. This directory is
gitignored — drop your own image here and do not commit it.

The ignored test `receipt_ocr_live_gemini` in `sync-server/src/lib.rs` reads a
hardcoded path. Change `RECEIPT_IMAGE` in that test if you use a different
filename.

```bash
GEMINI_API_KEY=your-key cargo test -p sync-server receipt_ocr_live -- --ignored --nocapture
```

Expected layout:

```
sync-server/testdata/receipt.jpg
```
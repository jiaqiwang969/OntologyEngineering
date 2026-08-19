# README visual assets

`engineering-ontology-cover.png` is the reader-facing preview used by the
repository README files. It is a deterministic render of page 1 of
`references/ontology-engineering-book/handbook/工程本体论-全书.pdf`; it is not an
independent book-cover source.

After an approved cover or book rebuild, regenerate the preview from the
repository root and inspect the resulting image before committing it:

```bash
pdftoppm -f 1 -l 1 -singlefile -png -r 144 \
  "references/ontology-engineering-book/handbook/工程本体论-全书.pdf" \
  docs/assets/engineering-ontology-cover
```

Keep the filename stable so both language versions of the README continue to
resolve the same reviewed image.

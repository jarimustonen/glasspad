# Brand assets

- `logo.png` / `logo.avif` — the glasspad mark: one frosted-glass tile with a
  rainbow refraction edge on a light gray-white background. `logo.md` is the
  prompt it was generated from and `logo.yaml` records the generation metadata.
- `social-preview.png` — the GitHub social-preview image (1280×640), uploaded
  under the repository's Settings → General → Social preview. It is rendered from
  `social-preview.html`, which uses `logo.png` and the palette and font stack
  from `DESIGN.md`. Text and mark stay inside the central 1120×480 area, because
  some surfaces crop the edges.

## Regenerating the social preview

Edit `social-preview.html`, then from the repository root run:

```bash
google-chrome --headless --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=1 --window-size=1280,640 \
  --screenshot=brand/social-preview.png brand/social-preview.html
```

The output must stay 1280×640 and under 1 MB, GitHub's limit. Look at the PNG
before committing it: the font stack falls back to whatever sans-serif the
rendering machine has, so line breaks can change from one machine to another.

# Introduction page images

Two placeholders used by the "See it" section of `docs/src/content/docs/introduction.md`:

| File | Should show |
| --- | --- |
| `before-template.png` | The `.docx` open in Word, with `{{placeholders}}` visible |
| `after-rendered.png` | The same document rendered, with real values in those spots |

Replace both files, keeping the names. Nothing else needs editing.

## What makes these read well

They sit **side by side** and each renders about **350px wide** on a desktop
screen, so a full page of A4 shrinks to the point of being unreadable.

- **Crop to the part that changes.** A letterhead and a few lines showing
  `{{customer_name}} → Acme Corp` communicates the idea; a whole page does not.
  The screenshots in `../template-syntax/` are cropped strips (around 800×92)
  for exactly this reason.
- **Give both images identical dimensions.** They're in a two-column grid, so
  matching sizes keeps the captions aligned. Around 900–1200px wide with the
  same height for both is a good target.
- **Frame both identically.** Same zoom, same region, same window chrome (or
  none). The only thing that should differ between them is the text — that
  difference is the entire point of the pair.
- **Use a realistic document.** The invoice in `docs/public/samples/invoice/`
  is a reasonable subject, and rendering it is one `docker compose up` away
  per the Getting started guide.

## Regenerating the placeholders

They were generated with Pillow; there's no build step and no script kept in
the repo. Any image at the right dimensions works — these exist only so the
page has something to lay out before real screenshots arrive.

# Slides and Presentations

Turn a Typst document into a slide deck where every heading is a slide, browse the slides as thumbnails, and present them full window with the keyboard.

Slides are a view of a Typst document, not a separate file type. The same `.typ` file stays your source, and its compiled PDF is still what you export. The Slides view is a quick outline of the deck built from your headings, so you can check structure and rehearse; it is not a rendering of your Typst page design. The **Slides (Typst)** template in [Templates](templates.md) gives you a deck to start from.

## 1. Switch to the Slides view

1. Open a Typst (`.typ`) document.
2. Click **Slides** in the **Text | Slides** switch at the right of the tab strip, or choose **View > Slides**. The same command is in the command palette (`Ctrl+K`, `Cmd+K`) as **Show document as slides**.
3. To go back, click **Text** or choose **View > Slides** again.

The view shows a column of thumbnails on the left and the current slide in the middle. The toolbar says which slide you are on, for example "Slide 2 of 5". The Slides view is available for Typst files only.

## 2. How slides are made

- Every heading starts a new slide, whatever its level. A top-level heading (`= Title`) is drawn as a title slide; deeper headings (`== Slide title`) are content slides.
- The paragraphs under a heading become the slide's text. List items are shown as plain text.
- The first display equation (a line of the form `$ ... $`) under a heading is shown on the slide as its source text.
- Lines that start with `#` (Typst code such as `#set` or `#figure`) are not shown in the Slides view.

If the document has no headings, the view says "This document has no slides yet" and offers **Back to text**. Add a heading such as `== My first slide` in the text view.

## 3. Move around the deck

- Click a thumbnail to select that slide.
- With focus on the current slide, press `Right Arrow` or `Page Down` for the next slide and `Left Arrow` or `Page Up` for the previous one.
- The thumbnail list is a keyboard list: use the arrow keys to move along it.

## 4. Present

1. In the Slides view, click **Present**, or press `F5`. You can also choose **Build > Present slides**, which switches to the Slides view first if you are in the text view.
2. The deck fills the window and shows only the current slide, with "Slide 3 of 5. Esc to leave." at the bottom.
3. Move with the keyboard:
   - **Next slide:** `Right Arrow`, `Space`, `Page Down` or `Enter`, or click the slide.
   - **Previous slide:** `Left Arrow`, `Page Up` or `Backspace`.
   - **Leave:** `Escape`.

Presenting needs at least one slide, and it works for Typst documents only.

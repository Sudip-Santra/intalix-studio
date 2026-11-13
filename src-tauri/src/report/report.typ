// Intalix Studio report layout.
//
// Everything comes from /report.json, written by src/report/pdf.rs: page setup, colours,
// header, table rows (already formatted), totals and footer. Report text is only ever read as
// data, so values from the database can't change the layout or run code.
//
// The layout mirrors the template preview (src/features/templates/ReportPreview.tsx).

#let d = json("/report.json")
#let c = d.colors
#let col(name) = rgb(c.at(name))

#let m = d.page.margin * 1mm
#let size = d.size * 1pt
// Room reserved at the top for the "Continued" line on pages after the first.
#let cont-h = size * 0.85 * 1.4 + 3mm
#let foot-h = size * 0.85 * 1.4 + 2mm

// Header and footer text with {page} / {pages} filled in.
#let fill-tokens(parts) = context {
  let out = ""
  for p in parts {
    if p == "{page}" {
      out += str(counter(page).get().first())
    } else if p == "{pages}" {
      out += str(counter(page).final().first())
    } else {
      out += p
    }
  }
  out
}

#set document(title: d.title)
#set text(
  font: d.font,
  size: size,
  fill: col("text"),
  lang: "en",
  hyphenate: false,
  number-width: "tabular",
)
#set par(leading: 0.42em, spacing: 0.6em)

#set page(
  width: d.page.w * 1mm,
  height: d.page.h * 1mm,
  margin: (
    x: m,
    top: m + (if d.header.visible { cont-h } else { 0mm }),
    bottom: m + (if d.footer.visible { foot-h + 3mm } else { 0mm }),
  ),
  header-ascent: 3mm,
  header: context {
    if d.header.visible and counter(page).get().first() > 1 {
      set text(size: 0.85em, fill: col("muted"))
      grid(columns: (1fr, auto), d.continued, [Continued])
    }
  },
  footer-descent: 3mm,
  footer: if d.footer.visible {
    set text(size: 0.85em, fill: col("muted"))
    block(
      width: 100%,
      stroke: if d.footer.rule { (top: 0.75pt + col("footerRule")) } else { none },
      inset: (top: 2mm),
      grid(
        columns: (1fr, 1fr, 1fr),
        column-gutter: 4mm,
        align(left, fill-tokens(d.footer.left)),
        align(center, fill-tokens(d.footer.center)),
        align(right, fill-tokens(d.footer.right)),
      ),
    )
  },
  background: if d.watermark.text != "" {
    place(center + horizon, rotate(-32deg, text(
      font: d.headingFont,
      size: 64pt,
      weight: "bold",
      tracking: 0.04em,
      fill: col("text").transparentize(100% - d.watermark.opacity * 1%),
      d.watermark.text,
    )))
  },
)

/* -------------------------------------------------------------------------------------------
 * Header (first page)
 * ----------------------------------------------------------------------------------------- */

#let hd = d.header
#let band = hd.style == "band"
#let fg = if band { col("onAccent") } else { col("text") }
#let muted = if band { col("onAccent") } else { col("muted") }
#let logo-size = hd.logoSize * 1pt

#let logo = if not hd.showLogo {
  none
} else if hd.logo != none {
  image(hd.logo, height: logo-size)
} else {
  box(
    width: logo-size,
    height: logo-size,
    fill: if band { col("onAccent") } else { col("accent") },
    align(center + horizon, text(
      font: d.headingFont,
      weight: "bold",
      size: logo-size * 0.38,
      fill: if band { col("accent") } else { col("onAccent") },
      hd.monogram,
    )),
  )
}

#let title-block = stack(
  spacing: 0.45em,
  // Measured to the descender, so the subtitle never touches letters like "y" or "p".
  text(font: d.headingFont, size: 2.05em, weight: "bold", fill: fg, bottom-edge: "descender", hd.title),
  ..if hd.subtitle != "" { (text(fill: muted, hd.subtitle),) } else { () },
)

#let header-row = if hd.layout == "center" {
  align(center, stack(spacing: 3mm, ..if logo != none { (logo,) } else { () }, title-block))
} else if hd.layout == "split" {
  if logo != none {
    grid(columns: (1fr, auto), column-gutter: 6mm, align: horizon, title-block, logo)
  } else { title-block }
} else {
  if logo != none {
    grid(columns: (auto, 1fr), column-gutter: 4mm, align: horizon, logo, title-block)
  } else { title-block }
}

#let parameters = if hd.showParameters and hd.parameters.len() > 0 {
  v(2mm)
  let items = hd.parameters.map(p => [#p.at(0): #text(weight: "semibold", fill: fg, p.at(1))])
  let line = text(size: 0.92em, fill: muted, items.join(h(6mm)))
  if hd.layout == "center" { align(center, line) } else { line }
}

#if hd.visible {
  // The space kept for the "Continued" line isn't needed on the first page.
  v(-cont-h)
  if band {
    pad(x: -m, top: -m, block(
      width: 100%,
      fill: col("accent"),
      inset: (x: m, y: m * 0.75),
      { header-row; parameters },
    ))
  } else {
    block(
      width: 100%,
      stroke: if hd.style == "rule" { (bottom: 1.5pt + col("accent")) } else { none },
      inset: (bottom: 3mm),
      { header-row; parameters },
    )
  }
  v(3mm)
}

/* -------------------------------------------------------------------------------------------
 * Table
 * ----------------------------------------------------------------------------------------- */

#let t = d.table
#let numbered = t.rowNumbers
#let ncols = t.columns.len() + (if numbered { 1 } else { 0 })
// Widths come from src/report/fit.rs, which sizes the columns to the page.
#let widths = (if numbered { (auto,) } else { () }) + t.columns.map(cl => cl.width * 1fr)
#let aligns = (if numbered { (right,) } else { () }) + t.columns.map(cl => if cl.right { right } else { left })
#let rule = 0.75pt + col("line")
#let head-fill = if t.style == "filled" { col("accent") } else if t.style == "tinted" { col("tint12") } else { none }
#let head-text = if t.style == "filled" { col("onAccent") } else { col("text") }
#let head-rule = if t.style == "tinted" {
  1.1pt + col("tint45")
} else if t.style == "underline" {
  1.1pt + col("accent")
} else if t.style == "plain" {
  1.1pt + col("text")
} else { none }
#let n = t.rows.len()
#let totals-y = if t.totals != none and n > 0 { n + 1 } else { -1 }

// A negative value arrives as a one-element array.
#let cell(v) = if type(v) == array {
  if d.negRed { text(fill: col("negative"), v.at(0)) } else { v.at(0) }
} else { v }

#let heading(label) = text(
  weight: "semibold",
  hyphenate: true,
  fill: head-text,
  ..if t.uppercase { (size: 0.88em, tracking: 0.03em) } else { (:) },
  if t.uppercase { upper(label) } else { label },
)

#let body = if n == 0 {
  (table.cell(colspan: ncols, text(fill: col("muted"), t.empty)),)
} else {
  t.rows.enumerate().map(((i, r)) => (
    (if numbered { (text(fill: col("muted"), str(i + 1)),) } else { () }) + r.map(cell)
  )).flatten()
}

#let totals = if totals-y > 0 {
  (if numbered { ([],) } else { () }) + t.totals.map(v => text(weight: "bold", cell(v)))
} else { () }

#set text(size: t.size * 1pt)
#table(
  columns: widths,
  align: (x, y) => aligns.at(x) + horizon,
  inset: (x: t.padX * 1mm, y: t.pad * 1mm),
  fill: (x, y) => if y == 0 {
    head-fill
  } else if y == totals-y {
    if t.style == "plain" { none } else { col("tint7") }
  } else if t.striped and calc.even(y) {
    col("stripe")
  } else { none },
  stroke: (x, y) => {
    let s = (:)
    if y == 0 {
      if head-rule != none { s.insert("bottom", head-rule) }
    } else if y == totals-y {
      s.insert("top", 1.1pt + col("text"))
    } else if t.grid != "none" {
      s.insert("bottom", rule)
    }
    if t.grid == "all" and x < ncols - 1 { s.insert("right", rule) }
    s
  },
  table.header(..(if numbered { (heading("#"),) } else { () }) + t.columns.map(cl => heading(cl.label))),
  ..body,
  ..totals,
)

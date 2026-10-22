# Worker wiring card contract

## Mandatory worker-page content

- Exact object names or unmistakable photos.
- Board orientation anchor.
- Wire color written as text and shown with color.
- Exact terminal marking as printed on the object.
- One complete block per repeated cable, probe, motor, valve, or sensor.
- Explicit shared power and ground grouping.
- Power-off instruction and fully qualified wrong-voltage warning.
- Physical labels to apply before wiring.
- Stop condition for a mismatched label, color, connector, or board revision.
- Final checkboxes for labels, signal pairs, polarity, and supervisor review.

## Information to keep off the worker page

- Protocol theory, bus addresses, register maps, firmware APIs, and research rationale.
- Unverified assumptions presented as facts.
- Abbreviations that are not printed on the hardware.
- Generic color conventions that conflict with the purchased cable label.
- An unexplained `VIN`, `VCC`, `5V`, or `3V3` when more than one object has a similarly named pin.

## Acceptance checklist

1. Every row in the frozen connection matrix appears exactly once in the drawing, except declared shared-bus summaries that are also expanded per endpoint.
2. Every drawn connection has a row in the matrix.
3. Wire color, signal function, terminal label, channel number, and object identity match the evidence.
4. Unused terminals and factory-default terminals are not conflated with proven electrical `NC` pins.
5. No live-work instruction is present.
6. The voltage-source warning names the source object.
7. The 300 dpi page is readable at actual A4 size and remains understandable without relying on color alone.
8. PDF fonts are embedded and the build log is clean.
9. A second reviewer can reproduce the wiring without verbal explanation.
10. First power remains conditional on physical inspection, continuity/polarity checks, and responsible approval.

## Preferred deliverables

```text
output/
  worker-wiring-card.pdf
source/
  worker-wiring-card.tex
  connection-matrix.csv
  evidence-register.csv
```

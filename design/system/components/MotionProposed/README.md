# MotionProposed

Intentional additions: M3 Expressive motion Madrigal does not ship yet, drawn here as the spec for later Slint work.

- Each stays inside the Slint limits: width, radius, colour, offset and weight animate; nothing scales.
- Spring overshoot can be approximated with Slint's `ease-out-back` or a `cubic-bezier`.
- All of them stop under reduced motion; a settings toggle should do the same in the app.

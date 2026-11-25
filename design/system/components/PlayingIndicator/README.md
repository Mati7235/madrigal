# PlayingIndicator

Three pill bars that bounce while a track plays. **Proposal**, and the fix for a contrast failure.

- Today the playing row sets its title in `primary` on `primary-container`: 4.20:1 in Light, under the 4.5:1 floor. The fix: title and metadata in `on-primary-container` (13.7:1), and "this is playing" carried by this indicator instead of by title colour.
- 14×14, three 4px pills, 2px gap, 900ms ease-in-out, staggered by 300ms; `primary` on surfaces, `on-primary-container` inside the playing row.
- Paused: the bars freeze at their rest heights. Reduced motion: frozen always.

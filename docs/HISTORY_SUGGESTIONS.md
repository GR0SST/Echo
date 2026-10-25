# Local suggestion frequency

History stores a saturating visit count per URL and local profile. Existing entries without the field represent one known visit; their earlier frequency cannot be reconstructed from the old deduplicated store. Private visits remain excluded. Title changes do not count as visits. Counts propagate through the existing shared history between normal and Blank windows, including updates with unchanged timestamps.

Typed Today results use frequency and age within their existing Space/section group. Favorites and Pinned retain their existing order. Split results retain their original matching pane representative. Empty-query results keep the established MRU behavior. Closed-history matches follow open-tab results and select only the bounded best candidates; the Library's stored chronological order is unchanged. Equal typed Today scores retain sidebar order, and equal history scores prefer recency.

The aggregate score caps frequency at 32 and decays it by days since the last visit. These weights are provisional: positive Arc references establish the ordering cases below, not Arc's unpublished complete scoring/typed-transition policy. Complete cross-Space ranking, bookmark behavior under Command-L and folder-specific results remain unqualified, so the umbrella parity row stays partial.

Chrome-like and Firefox readers retain source visit totals when available and accept older schemas without the column. Safari keeps the greater of its stored total and retained visit rows. Repeated imports merge totals with max rather than inflating them, while preserving the newer title/date. Reader checks verify source database bytes remain unchanged.

## Arc references

The closed-history reference has a Pinned result, one open tab, a six-visit closed page and a newer one-visit closed page. Arc orders them Pinned → Open → Frequent → Recent. Its filtered read-only SQLite capture records the six/one totals: `target/parity/command-sidebar-r2/ranking-history-immutable.json`. Inspected captures: `target/parity/history-ranking/arc-closed-{light,dark}.png`.

A fresh guarded Echo Test session visits one page six times, opens a newer page once, then searches while both remain open. Arc ranks Frequent above Recent despite their opposite sidebar order. Evidence: `target/parity/history-ranking/arc-open-dark.png` and `target/parity/reaudit2/sessions/open-frequency-ranking-functional/reference.json`. The SQLite result in that fresh capture is empty; the six navigations are established by the scripted title-confirmed sequence, not by a claimed database total. The root and owned tabs are closed, with no owned windows, held mouse button or auxiliary layers left. An initial zero-window Cmd-N attempt was retried with an immediate front-process check; Arc was not restarted.

## Acceptance

Core tests cover count persistence, old-session defaults, saturation, aging, bounded results and unchanged Library order. The existing import transaction test also verifies repeated imports retain the total. GPUI tests type and select ranked results, preserve Favorites/Pinned order and empty MRU, reuse the selected existing tab, and publish frequency-only changes across Blank/normal history. An import-reader matrix covers Chrome, Firefox and Safari totals and unchanged source files.

Native disposable-profile runs perform six actual navigations and verify both the fixture's request count and the persisted total. Closed case order is Pinned → Open → Frequent → Recent; open case order is Frequent → Recent. Both quit natively. Inspected Echo captures and machine-readable acceptance: `target/parity/history-ranking/native-{closed,open}/results-screen.png`, `result.json`, and logs. They qualify functional order; remaining glyph/material differences are deferred.

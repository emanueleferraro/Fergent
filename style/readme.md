# fergents — design system

Cartella di riferimento visivo per il progetto **fergents**. Contiene i token,
i componenti CSS e le pagine HTML di documentazione. Serve come guida per
mantenere coerente lo stile quando si generano nuove UI.

---

## File presenti

### `tokens.json`

Fonte di verità dei design token. Contiene colori, radii, spacing, tipografia,
motion, ombre, blur e z-index, tutti in formato JSON strutturato.

Se un valore va cambiato, si cambia **qui per primo**, poi si aggiorna `tokens.css`
con lo stesso valore.

### `tokens.css`

Gli stessi token di `tokens.json` esposti come CSS custom properties su `:root`.
Include anche un blocco di **alias semantici** (`--surface-1`, `--text-strong`, ecc.)
da preferire nel codice applicativo.

Va incluso per primo, prima di qualsiasi altro CSS.

### `typography.css`

Scale tipografiche e utility di testo: `.t-display`, `.t-h1`, `.t-body`,
`.t-caption`, `.t-mono`, classi di colore, pesi, allineamento, truncate,
clamp, stili per link e code block.

Dipende da `tokens.css`.

### `components.css`

Tutti i componenti riutilizzabili del sistema:

- Bottoni (varianti, taglie, stati, loading)
- Input, textarea, select, field wrapper, stati errore/successo
- Checkbox, radio, switch
- Card, metric, badge, chip
- Progress, spinner, dots, skeleton
- Toast (success/error/warning/info)
- Tabs (pill e underline)
- Breadcrumb, pagination
- Tabelle
- Avatar (taglie, colori, stack, status)
- Feed
- Modal, dropdown, tooltip
- Empty state, inline feedback
- Success ring animato
- Window/panel
- Layout helpers (`.panel`, `.panel-row`, `.panel-stack`)

**Phase 1 additions (brain UI):**

- Sidebar sections (`.sidebar`, `.sidebar-head`, `.sidebar-section`, `.sidebar-section__title`) — context-sensitive sidebars
- Tree / explorer (`.tree`, `.tree-row`, `.tree-caret`, `.tree-icon`) — files and tags
- Tag list (`.tag-list`, `.tag`, `.tag-count`)
- Search results (`.result-list`, `.result-item`, `.result-snippet`, `.mark`, `.result-score`)
- Backlinks & outline (`.link-list`, `.link-item`, `.outline-list`, `.outline-item`)
- Markdown body (`.md`, `.wikilink`, `.callout` note/tip/warning/danger)
- Keyboard (`.kbd`, `.kbd-group`)
- Settings (`.settings`, `.settings-section`, `.settings-row`, `.settings-control`)
- Graph view (`.graph-shell`, `.graph-canvas`, `.graph-toolbar`, `.graph-legend`)

Dipende da `tokens.css` e `typography.css`.

### `styleguide.html`

Living styleguide navigabile. Mostra in un'unica pagina:

- Palette completa con swatch
- Scale tipografiche
- Spacing e radii
- Motion
- Tutti i componenti con esempi visivi

Serve per verificare che una modifica ai token non rompa nulla e per
consultare rapidamente come si usa un componente.

Include `tokens.css`, `typography.css`, `components.css`.

### `examples.html`

Esempi di interfacce reali costruite con il sistema:

1. Dashboard workspace (sidebar, KPI, grafico a barre, tabella)
2. Login form (validazione inline, checkbox, bottone loading)
3. Notification center (i 4 toast semantici)
4. Empty state con CTA
5. Success flow con ring animato

Serve come base da copiare per nuovi layout.

Include gli stessi CSS di `styleguide.html`.

### `patterns.html`

Quindici pattern complessi, ognuno autocontenuto e costruito sopra i componenti
base. Ogni pattern ha la sua sezione con anchor.

1. **Chat agentica** — bolle user/assistant/tool/system, thinking collassabile, tool-call inline, streaming cursor, allegati, composer
2. **Diff viewer** — vista inline + side-by-side, azioni approva/rifiuta
3. **Permission bar** — 3 stati (manual/auto/full) con colori semantici
4. **Editor panels** — file tree, tab bar, breadcrumb, area codice con syntax highlight
5. **Terminale** — tab shell/dev/tests, output colorato, cursore animato
6. **Workflow canvas** — nodi, edge SVG, porte, minimappa, inspector
7. **Brain / Graph view** — nodi entità, edge relazioni, filtri laterali
8. **Command palette** — overlay, ricerca fuzzy, categorie, scorciatoie
9. **Sidebar app** — rail icon-only collassabile + pannello espanso
10. **Status bar** — modello, token, costi, sync, agenti, permessi
11. **Agent status** — lista agenti con stati idle/working/waiting/done + sub-agenti
12. **Plugin cards** — icona, permessi, source, versione, stato
13. **Dashboard widgets** — griglia drag&drop con handle
14. **Tabs workspace** — tab sessione/progetto con close on hover
15. **Split panes** — verticale e orizzontale con gutter ridimensionabile

Include gli stessi CSS di `styleguide.html`.

---

## Ordine di inclusione

```html
<link rel="stylesheet" href="tokens.css">
<link rel="stylesheet" href="typography.css">
<link rel="stylesheet" href="components.css">
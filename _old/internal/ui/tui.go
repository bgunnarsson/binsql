package ui

import (
	"context"
	"fmt"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"

	"github.com/bgunnarsson/binsql/internal/db"
)

type uiState struct {
	ctx    context.Context
	db     db.DB
	label  string
	app    *tview.Application
	pages  *tview.Pages
	tables *tview.List

	result   *tview.Table
	query    *tview.InputField
	status   *tview.TextView
	lastRows *db.Rows
}

// Run starts the interactive TUI using tview/tcell.
func Run(ctx context.Context, sdb db.DB, label string) error {
	state := &uiState{
		ctx:   ctx,
		db:    sdb,
		label: label, // driver name, e.g. "sqlite"
		app:   tview.NewApplication(),
	}

	state.setupTheme()
	root := state.buildLayout()

	state.app.
		SetRoot(root, true).
		EnableMouse(true)

	// initial focus on tables pane
	state.app.SetFocus(state.tables)

	// Global keybindings – all stay on the UI goroutine.
	state.app.SetInputCapture(func(ev *tcell.EventKey) *tcell.EventKey {
		frontName, _ := state.pages.GetFrontPage()
		focus := state.app.GetFocus()

		// When an overlay is open, ESC/Enter/Ctrl+Q/Ctrl+/ close it.
		if frontName == "rowDetail" || frontName == "help" {
			switch {
			case ev.Key() == tcell.KeyEsc,
				ev.Key() == tcell.KeyEnter,
				isCtrlKey(ev, tcell.KeyCtrlQ, 'q'),
				isCtrlKey(ev, 0, '/'):
				state.pages.RemovePage(frontName)
				state.app.SetFocus(state.result)
				return nil
			}
			return ev
		}

		// Vim-style pane navigation (Ctrl+h/j/k/l)
		switch {
		case isCtrlKey(ev, tcell.KeyCtrlH, 'h'): // left
			state.app.SetFocus(state.tables)
			return nil
		case isCtrlKey(ev, tcell.KeyCtrlL, 'l'): // right
			state.app.SetFocus(state.result)
			return nil
		case isCtrlKey(ev, tcell.KeyCtrlJ, 'j'): // down
			state.app.SetFocus(state.query)
			return nil
		case isCtrlKey(ev, tcell.KeyCtrlK, 'k'): // up
			state.app.SetFocus(state.status)
			return nil
		}

		switch {
		// Quit: Ctrl+Q or Ctrl+C
		case isCtrlKey(ev, tcell.KeyCtrlQ, 'q') || ev.Key() == tcell.KeyCtrlC:
			state.app.Stop()
			return nil

		// Focus query: Ctrl+:
		// (Ctrl+Shift+; on a US layout – rune ':' with Ctrl)
		case isCtrlKey(ev, 0, ':') && focus != state.query:
			state.app.SetFocus(state.query)
			return nil

		// Reload tables: Ctrl+R
		case isCtrlKey(ev, tcell.KeyCtrlR, 'r'):
			_ = state.loadTables()
			return nil

		// Help: Ctrl+/
		case isCtrlKey(ev, 0, '/'):
			state.toggleHelp()
			return nil

		// Row expand: Enter while focused on results
		case ev.Key() == tcell.KeyEnter && focus == state.result:
			state.expandCurrentRow()
			return nil
		}

		// Let widgets handle the key normally.
		return ev
	})

	// Initial data load (synchronous, safe before Run).
	_ = state.loadTables()

	return state.app.Run()
}

// Catppuccin Mocha theme.
// - Borders (all, including tables): #595B72
// - Titles (section captions): cyan #89DCEB
// - BINSQL <driver> text keeps its own accent color in buildLayout.
func (s *uiState) setupTheme() {
	// Mocha base colors
	// base:      #1E1E2E (30, 30, 46)
	// surface0:  #313244 (49, 50, 68)
	// surface1:  #45475A (69, 71, 90)
	// text:      #CDD6F4 (205, 214, 244)
	// subtext0:  #A6ADC8 (166, 173, 200)
	// overlay2:  #9399B2 (147, 153, 178)
	// borders:   #595B72 (89, 91, 114)
	// cyan:      #89DCEB (137, 220, 235)

	tview.Styles.PrimitiveBackgroundColor = tcell.NewRGBColor(30, 30, 46)    // base
	tview.Styles.ContrastBackgroundColor = tcell.NewRGBColor(49, 50, 68)     // surface0
	tview.Styles.MoreContrastBackgroundColor = tcell.NewRGBColor(69, 71, 90) // surface1

	// all borders (frames, table borders, separators)
	tview.Styles.BorderColor = tcell.NewRGBColor(89, 91, 114)

	tview.Styles.PrimaryTextColor = tcell.NewRGBColor(205, 214, 244) // text
	tview.Styles.SecondaryTextColor = tcell.NewRGBColor(166, 173, 200)
	tview.Styles.TertiaryTextColor = tcell.NewRGBColor(147, 153, 178)

	// section titles cyan
	tview.Styles.TitleColor = tcell.NewRGBColor(137, 220, 235)

	// graphics (lines / separators) same as border color
	tview.Styles.GraphicsColor = tcell.NewRGBColor(89, 91, 114)
}

func (s *uiState) buildLayout() tview.Primitive {
	// Connection header: "BINSQL <DRIVER>" with existing accent color.
	header := tview.NewTextView().
		SetTextAlign(tview.AlignLeft).
		SetDynamicColors(true).
		SetText(fmt.Sprintf("[::b]BINSQL[-]  [#C0A1F0]%s[-]", strings.ToUpper(s.label)))

	header.SetBorder(true)
	header.SetBorderPadding(0, 0, 1, 1)
	header.SetTitle(" Connection ")

	// TABLE LIST
	s.tables = tview.NewList().
		ShowSecondaryText(false)
	s.tables.SetBorder(true)
	s.tables.SetTitle(" Tables ")
	s.tables.SetDoneFunc(func() {
		// ESC in list -> focus query.
		s.app.SetFocus(s.query)
	})
	s.tables.SetSelectedFunc(func(index int, mainText string, secondaryText string, shortcut rune) {
		table := mainText
		if table == "" {
			return
		}
		sql := fmt.Sprintf("SELECT * FROM %s LIMIT 100", table)
		s.query.SetText(sql)
		s.runQuery(sql) // synchronous
	})

	// HELP BOX under tables, no title.
	helpBox := tview.NewTextView().
		SetDynamicColors(true).
		SetTextAlign(tview.AlignLeft).
		SetText(" Help: Ctrl+?")
	helpBox.SetBorder(true)

	// RESULT TABLE
	s.result = tview.NewTable().
		SetBorders(true). // show grid
		SetFixed(1, 0)
	s.result.SetBorder(true)
	s.result.SetTitle(" Results ")
	s.result.SetSelectable(true, true) // move across cells

	// QUERY INPUT
	s.query = tview.NewInputField().
		SetLabel("> ").
		SetFieldWidth(0) // grow with window
	s.query.SetBorder(true)
	s.query.SetTitle(" Query (Enter to run) ")
	s.query.SetDoneFunc(func(key tcell.Key) {
		if key == tcell.KeyEnter {
			sql := strings.TrimSpace(s.query.GetText())
			if sql == "" {
				return
			}
			s.runQuery(sql) // synchronous
		}
	})

	// STATUS BAR
	s.status = tview.NewTextView().
		SetDynamicColors(true).
		SetTextAlign(tview.AlignLeft)
	s.status.SetBorder(true)
	s.status.SetTitle(" Status ")

	left := tview.NewFlex().
		SetDirection(tview.FlexRow).
		AddItem(header, 3, 0, false).
		AddItem(s.tables, 0, 1, true).
		AddItem(helpBox, 3, 0, false)

	main := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(s.result, 0, 1, false).
		AddItem(s.query, 3, 0, false).
		AddItem(s.status, 3, 0, false)

	content := tview.NewFlex().
		AddItem(left, 30, 0, true).
		AddItem(main, 0, 1, false)

	s.pages = tview.NewPages().
		AddPage("main", content, true, true)

	return s.pages
}

func (s *uiState) loadTables() error {
	s.setStatus("[yellow]Loading tables…[-]")

	tables, err := s.db.ListTables(s.ctx)
	if err != nil {
		s.setStatus(fmt.Sprintf("[red]Error loading tables: %v[-]", err))
		return err
	}

	s.tables.Clear()
	for _, t := range tables {
		name := strings.TrimSpace(t)
		if name != "" {
			s.tables.AddItem(name, "", 0, nil)
		}
	}

	if s.tables.GetItemCount() == 0 {
		s.setStatus("[gray]No tables found.[-]")
	} else {
		s.tables.SetCurrentItem(0)
		s.setStatus("[green]Tables loaded. Use arrows + Enter, or type a query below.[-]")
	}

	return nil
}

func (s *uiState) runQuery(sql string) {
	start := time.Now()
	s.setStatus(fmt.Sprintf("[yellow]Running query…[-] [gray]%s[-]", truncateInline(sql, 80)))

	rows, err := s.db.Query(s.ctx, sql)
	if err != nil {
		s.setStatus(fmt.Sprintf("[red]Query error:[-] %v", err))
		return
	}

	elapsed := time.Since(start)
	s.renderRows(rows)
	s.setStatus(fmt.Sprintf(
		"[green]Query OK[-] [gray](%d rows, %s)[-]",
		len(rows.Data),
		elapsed.Truncate(time.Millisecond),
	))
}

func (s *uiState) renderRows(rows *db.Rows) {
	s.result.Clear()
	s.lastRows = rows

	if len(rows.Columns) == 0 {
		return
	}

	const maxColWidth = 40

	colCount := len(rows.Columns)
	colWidths := make([]int, colCount)

	// base width from headers
	for i, col := range rows.Columns {
		colWidths[i] = runeLen(col.Name)
		if colWidths[i] > maxColWidth {
			colWidths[i] = maxColWidth
		}
	}

	// refine widths from data (up to some rows)
	rowLimit := len(rows.Data)
	if rowLimit > 200 {
		rowLimit = 200
	}
	for r := 0; r < rowLimit; r++ {
		row := rows.Data[r]
		for c := 0; c < colCount && c < len(row); c++ {
			text := formatValue(row[c])
			l := runeLen(text)
			if l > maxColWidth {
				l = maxColWidth
			}
			if l > colWidths[c] {
				colWidths[c] = l
			}
		}
	}

	// header (no special background color – use base theme)
	for colIdx, col := range rows.Columns {
		headerText := padRight(col.Name, colWidths[colIdx])
		cell := tview.NewTableCell(headerText).
			SetAlign(tview.AlignLeft).
			SetSelectable(false).
			SetAttributes(tcell.AttrBold)
		s.result.SetCell(0, colIdx, cell)
	}

	// data
	for rIdx, row := range rows.Data {
		for cIdx := 0; cIdx < colCount && cIdx < len(row); cIdx++ {
			text := formatValue(row[cIdx])

			truncated := text
			if runeLen(truncated) > maxColWidth {
				truncated = truncateRunes(truncated, maxColWidth-1) + "…"
			}
			display := padRight(truncated, colWidths[cIdx])

			align := tview.AlignLeft
			if looksNumeric(text) {
				align = tview.AlignRight
			}

			cell := tview.NewTableCell(display).
				SetAlign(align).
				SetSelectable(true)

			// zebra striping on a slightly darker Mocha background (mantle: #181825)
			if rIdx%2 == 1 {
				cell.SetBackgroundColor(tcell.NewRGBColor(24, 24, 37))
			}

			s.result.SetCell(rIdx+1, cIdx, cell)
		}
	}

	s.result.ScrollToBeginning()
}

func (s *uiState) expandCurrentRow() {
	if s.lastRows == nil || len(s.lastRows.Data) == 0 {
		return
	}

	rowIdx, _ := s.result.GetSelection()
	if rowIdx <= 0 {
		return // header
	}
	rowIdx-- // adjust for header row

	if rowIdx < 0 || rowIdx >= len(s.lastRows.Data) {
		return
	}

	var b strings.Builder
	b.Grow(256)

	for i, col := range s.lastRows.Columns {
		b.WriteString(col.Name)
		b.WriteString(":\n")

		val := ""
		if i < len(s.lastRows.Data[rowIdx]) {
			val = formatValue(s.lastRows.Data[rowIdx][i])
		}
		if val == "" {
			val = "NULL"
		}
		b.WriteString("  ")
		b.WriteString(val)
		b.WriteString("\n\n")
	}

	text := tview.NewTextView().
		SetText(b.String()).
		SetScrollable(true).
		SetWrap(true).
		SetWordWrap(true)
	text.SetDynamicColors(false)

	header := tview.NewTextView().
		SetTextAlign(tview.AlignCenter).
		SetText(" Row detail (ESC/Enter/Ctrl+Q/Ctrl+/ to close) ")
	header.SetDynamicColors(false)

	layout := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(header, 3, 0, false).
		AddItem(text, 0, 1, true)

	frame := tview.NewFrame(layout).
		SetBorders(0, 0, 1, 1, 1, 1)
	frame.SetBorder(true).
		SetTitle(" Row detail ").
		SetTitleAlign(tview.AlignLeft)

	// center-ish overlay
	modal := tview.NewFlex().
		AddItem(nil, 0, 1, false).
		AddItem(
			tview.NewFlex().SetDirection(tview.FlexRow).
				AddItem(nil, 0, 1, false).
				AddItem(frame, 0, 3, true).
				AddItem(nil, 0, 1, false),
			0, 3, true,
		).
		AddItem(nil, 0, 1, false)

	s.pages.AddAndSwitchToPage("rowDetail", modal, true)
	s.app.SetFocus(text)
}

func (s *uiState) toggleHelp() {
	frontName, _ := s.pages.GetFrontPage()
	if frontName == "help" {
		s.pages.RemovePage("help")
		s.app.SetFocus(s.result)
		return
	}
	s.showHelp()
}

func (s *uiState) showHelp() {
	const helpText = `
[::b]Global[-]
  Ctrl+Q / Ctrl+C   Quit
  Ctrl+/            Toggle this help

[::b]Navigation[-]
  ↑ / ↓             Move in lists/tables
  Ctrl+h            Focus tables (left)
  Ctrl+l            Focus results (right)
  Ctrl+j            Focus query (down)
  Ctrl+k            Focus status (up)

[::b]Tables pane[-]
  Enter             SELECT * FROM <table> LIMIT 100

[::b]Results pane[-]
  Enter             Expand current row

[::b]Query input[-]
  Enter             Run SQL in the input
  Ctrl+:            Focus query from anywhere

[::b]Notes[-]
  Mouse support is enabled (scroll, click).
  Overlays close with ESC, Enter, Ctrl+Q, or Ctrl+/.`

	txt := tview.NewTextView().
		SetDynamicColors(true).
		SetTextAlign(tview.AlignLeft).
		SetScrollable(true).
		SetWrap(true).
		SetWordWrap(true)
	txt.SetText(helpText)

	header := tview.NewTextView().
		SetTextAlign(tview.AlignCenter).
		SetDynamicColors(true).
		SetText("[::b]binsql help (ESC/Enter/Ctrl+Q/Ctrl+/ to close)[-]")

	layout := tview.NewFlex().SetDirection(tview.FlexRow).
		AddItem(header, 3, 0, false).
		AddItem(txt, 0, 1, true)

	frame := tview.NewFrame(layout).
		SetBorders(0, 0, 1, 1, 1, 1)
	frame.SetBorder(true).
		SetTitle(" Help ").
		SetTitleAlign(tview.AlignLeft)

	modal := tview.NewFlex().
		AddItem(nil, 0, 1, false).
		AddItem(
			tview.NewFlex().SetDirection(tview.FlexRow).
				AddItem(nil, 0, 1, false).
				AddItem(frame, 0, 3, true).
				AddItem(nil, 0, 1, false),
			0, 3, true,
		).
		AddItem(nil, 0, 1, false)

	s.pages.AddAndSwitchToPage("help", modal, true)
	s.app.SetFocus(txt)
}

// setStatus updates the status bar text.
func (s *uiState) setStatus(msg string) {
	if s.status == nil {
		return
	}
	s.status.SetText(msg)
}

// isCtrlKey checks for Ctrl+<ch>, handling both KeyCtrlX and rune+modifier.
func isCtrlKey(ev *tcell.EventKey, key tcell.Key, ch rune) bool {
	if key != 0 && ev.Key() == key {
		return true
	}
	return ev.Rune() == ch && (ev.Modifiers()&tcell.ModCtrl) != 0
}

func formatValue(v any) string {
	if v == nil {
		return "NULL"
	}
	switch val := v.(type) {
	case []byte:
		if len(val) > 256 {
			return fmt.Sprintf("[blob %d bytes]", len(val))
		}
		return string(val)
	default:
		return fmt.Sprint(val)
	}
}

func truncateInline(s string, max int) string {
	if max <= 0 || len(s) <= max {
		return s
	}
	if max <= 3 {
		return s[:max]
	}
	return s[:max-3] + "..."
}

// runeLen counts runes so we don’t under/over-pad UTF-8 text.
func runeLen(s string) int {
	return utf8.RuneCountInString(s)
}

func truncateRunes(s string, n int) string {
	if n <= 0 {
		return ""
	}
	if runeLen(s) <= n {
		return s
	}
	var b strings.Builder
	b.Grow(len(s))
	i := 0
	for _, r := range s {
		if i >= n {
			break
		}
		b.WriteRune(r)
		i++
	}
	return b.String()
}

func padRight(s string, width int) string {
	rl := runeLen(s)
	if rl >= width {
		return s
	}
	var b strings.Builder
	b.Grow(len(s) + (width-rl))
	b.WriteString(s)
	for i := 0; i < width-rl; i++ {
		b.WriteRune(' ')
	}
	return b.String()
}

func looksNumeric(s string) bool {
	s = strings.TrimSpace(s)
	if s == "" {
		return false
	}
	hasDigit := false
	for i, r := range s {
		if r == '+' || r == '-' {
			if i != 0 {
				return false
			}
			continue
		}
		if r == '.' || r == ',' {
			continue
		}
		if unicode.IsDigit(r) {
			hasDigit = true
			continue
		}
		return false
	}
	return hasDigit
}


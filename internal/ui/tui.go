package ui

import (
	"context"
	"fmt"
	"strconv"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	textinput "github.com/charmbracelet/bubbles/textinput"
	viewport "github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/lipgloss"

	"github.com/bgunnarsson/binsql/internal/db"
)

// Palette
const (
	colorBg         = "#0F0F20"
	colorBorder     = "#4A2366"
	colorAccentDeep = "#782E5C"
	colorAccent1    = "#D352FF"
	colorAccent2    = "#E48EFF"
	colorText       = "#E5E7F5"
	colorMuted      = "#8B8FA7"
	colorError      = "#FF5C7C"
)

// Styles
var (
	titleStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorAccent1)).
			Bold(true)

	dbBadgeStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorAccent2)).
			Bold(true)

	headerHelpStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorMuted))

	hintStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorMuted))

	metaEchoStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorAccent2))

	errorStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorError))

	tableBorderStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color(colorBorder))

	tableHeaderStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color(colorAccent2)).
				Bold(true)

	tableBodyStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorMuted))

	promptStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorAccent2)).
			Bold(true)

	inputTextStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorText))

	footerStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorMuted))

	rootStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorText))
)

// Model is the Bubble Tea TUI model.
type Model struct {
	ctx context.Context
	db  db.DB

	history []string        // line-based output log
	input   textinput.Model // prompt

	width  int
	height int

	lastResult *db.Rows

	cmdHistory []string
	cmdIndex   int

	vp viewport.Model // scrollable content area
}

func NewModel(ctx context.Context, d db.DB) Model {
	ti := textinput.New()
	ti.Prompt = promptStyle.Render("BINSQL> ")
	ti.TextStyle = inputTextStyle
	ti.Focus()

	vp := viewport.New(0, 0)
	// we want PgUp/PgDn + mouse wheel; viewport already handles MouseMsg

	return Model{
		ctx:     ctx,
		db:      d,
		history: []string{},
		input:   ti,
		vp:      vp,
	}
}

func (m Model) Init() tea.Cmd {
	return textinput.Blink
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmd tea.Cmd
	handled := false

	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "ctrl+c":
			return m, tea.Quit

		// Up/Down: command history only (no scrolling)
		case "up":
			if strings.TrimSpace(m.input.Value()) == "" {
				m.historyPrev()
				handled = true
			}
		case "down":
			if strings.TrimSpace(m.input.Value()) == "" {
				m.historyNext()
				handled = true
			}

		// PgUp/PgDn: scroll central area
		case "pgup":
			if m.vp.Height > 0 {
				m.vp.LineUp(m.vp.Height / 2)
			} else {
				m.vp.LineUp(5)
			}
			handled = true

		case "pgdown":
			if m.vp.Height > 0 {
				m.vp.LineDown(m.vp.Height / 2)
			} else {
				m.vp.LineDown(5)
			}
			handled = true

		case "enter":
			line := strings.TrimSpace(m.input.Value())
			if line != "" {
				m.pushCmd(line)
				cmd = m.execute(line)
			}
			m.input.SetValue("")
			m.input.CursorEnd()
			handled = true
		}

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height

		// Layout:
		//   1 line header
		//   1 line help under header
		//   content viewport
		//   1 blank line
		//   1 line prompt
		//   1 line footer help
		headerLines := 2
		//blankBeforePrompt := 1
		promptLines := 1
		//footerLines := 1

		used := headerLines +  promptLines
		vpHeight := msg.Height - used
		if vpHeight < 3 {
			vpHeight = 3
		}

		m.vp.Width = msg.Width
		m.vp.Height = vpHeight
		m.refreshViewport()
	}

	// Let viewport see mouse / other events (scroll wheel, etc.)
	var vpCmd tea.Cmd
	m.vp, vpCmd = m.vp.Update(msg)
	if vpCmd != nil {
		cmd = tea.Batch(cmd, vpCmd)
	}

	if !handled {
		// Rest goes to text input (typing, left/right, etc.)
		var tiCmd tea.Cmd
		m.input, tiCmd = m.input.Update(msg)
		if tiCmd != nil {
			cmd = tea.Batch(cmd, tiCmd)
		}
	}

	return m, cmd
}

func (m Model) View() string {
	var b strings.Builder

	// HEADER BAR
	headerLeft := titleStyle.Render("BINSQL") + " " + dbBadgeStyle.Render("[sqlite]")
	headerRight := headerHelpStyle.Render("\\dt tables  ·  \\e [n] expand  ·  \\q quit  ·  ↑/↓ history")
	header := padToWidth(headerLeft, headerRight, m.width)

	b.WriteString(header)
	b.WriteString("\n")

	// Help line under header
	// b.WriteString(hintStyle.Render(`PgUp/PgDn scroll`))
	// b.WriteString("\n")

	// CONTENT AREA (scrollable)
	b.WriteString(m.vp.View())
	b.WriteString("\n")

	// Prompt + footer
	b.WriteString(m.input.View())
	b.WriteString("\n")
	// b.WriteString(
	// 	footerStyle.Render("^C Quit  ·  SQL + Enter to run"),
	// )
	// b.WriteString("\n")

	return rootStyle.Render(b.String())
}

// ---------- history + viewport helpers ----------

func (m *Model) refreshViewport() {
	m.vp.SetContent(strings.Join(m.history, "\n"))
	// When we add new stuff we want to follow the tail
	m.vp.GotoBottom()
}

func (m *Model) appendLines(lines ...string) {
	m.history = append(m.history, lines...)
	if len(m.history) > 1000 {
		m.history = m.history[len(m.history)-1000:]
	}
	if m.vp.Width > 0 && m.vp.Height > 0 {
		m.refreshViewport()
	}
}

func (m *Model) appendStyled(line string, style lipgloss.Style) {
	m.appendLines(style.Render(line))
}

// ---------- command execution ----------

func (m *Model) execute(line string) tea.Cmd {
	m.appendStyled(">>> "+line, metaEchoStyle)

	if strings.HasPrefix(line, "\\") {
		return m.runMeta(line)
	}

	m.runSQL(line)
	return nil
}

func (m *Model) runMeta(cmd string) tea.Cmd {
	s := strings.TrimSpace(cmd)

	switch {
	case s == `\q`:
		m.appendStyled("Bye.", hintStyle)
		return tea.Quit

	case s == `\dt`:
		m.listTables()
		return nil

	case strings.HasPrefix(s, `\e`):
		m.expandRow(s)
		return nil

	default:
		m.appendStyled("Unknown command: "+cmd, errorStyle)
		return nil
	}
}

// \lt – boxed table
func (m *Model) listTables() {
	// First try the original SQLite-specific query – nicer output when it works.
	const sqliteQuery = `
		SELECT
			'' AS "Schema",
			name AS "Name",
			CASE
				WHEN name LIKE 'sqlite_%' THEN 'SYSTEM TABLE'
				ELSE UPPER(type)
			END AS "Type"
		FROM sqlite_master
		WHERE type IN ('table','view')
		ORDER BY
			CASE WHEN name LIKE 'sqlite_%' THEN 0 ELSE 1 END,
			name;
	`

	rows, err := m.db.Query(m.ctx, sqliteQuery)
	if err != nil {
		// Probably not SQLite (e.g. Postgres) – fall back to the driver’s ListTables.
		names, err2 := m.db.ListTables(m.ctx)
		if err2 != nil {
			m.appendStyled("error listing tables: "+err2.Error(), errorStyle)
			return
		}
		if len(names) == 0 {
			m.appendLines("(no relations)")
			return
		}

		// Build a generic single-column result set so we can reuse renderBoxTable.
		rows = &db.Rows{
			Columns: []db.Column{
				{Name: "Table", Type: ""},
			},
			Data: make([]db.Row, len(names)),
		}
		for i, name := range names {
			rows.Data[i] = db.Row{name}
		}
	}

	if len(rows.Data) == 0 {
		m.appendLines("(no relations)")
		return
	}

	maxColWidth := 40
	if m.width > 0 {
		if w := (m.width - 10) / len(rows.Columns); w > 8 {
			maxColWidth = w
		}
	}

	lines := renderBoxTable(rows, maxColWidth)

	m.appendStyled("List of relations", hintStyle)
	for i, line := range lines {
		switch i {
		case 1:
			m.appendLines(tableHeaderStyle.Render(line))
		case 0, 2, len(lines)-1:
			m.appendLines(tableBorderStyle.Render(line))
		default:
			m.appendLines(tableBodyStyle.Render(line))
		}
	}
	m.appendStyled(fmt.Sprintf("(%d rows)", len(rows.Data)), hintStyle)
}

// Standard SELECT overview – boxed table
func (m *Model) runSQL(sqlStr string) {
	rows, err := m.db.Query(m.ctx, sqlStr)
	if err != nil {
		m.appendStyled("error: "+err.Error(), errorStyle)
		return
	}

	m.lastResult = rows

	if len(rows.Columns) == 0 {
		m.appendLines("(no columns)")
		return
	}

	if len(rows.Data) > 0 {
		m.appendStyled(`Use \e [rowNumber] to expand a row (example: \e 1).`, hintStyle)
	}

	maxColWidth := 40
	if m.width > 0 {
		if w := (m.width - 10) / len(rows.Columns); w > 8 {
			maxColWidth = w
		}
	}

	lines := renderBoxTable(rows, maxColWidth)

	for i, line := range lines {
		switch i {
		case 1:
			m.appendLines(tableHeaderStyle.Render(line))
		case 0, 2, len(lines) - 1:
			m.appendLines(tableBorderStyle.Render(line))
		default:
			m.appendLines(tableBodyStyle.Render(line))
		}
	}

	m.appendStyled(fmt.Sprintf("(%d rows)", len(rows.Data)), hintStyle)
}

// \e / \e 3 – expand row
func (m *Model) expandRow(cmd string) {
	if m.lastResult == nil || len(m.lastResult.Data) == 0 {
		m.appendStyled("no previous result to expand", errorStyle)
		return
	}

	s := strings.TrimSpace(cmd)
	parts := strings.Fields(s)

	idx := 1
	if len(parts) > 1 {
		n, err := strconv.Atoi(parts[1])
		if err != nil || n < 1 || n > len(m.lastResult.Data) {
			m.appendStyled("usage: \\e [rowNumber]", errorStyle)
			return
		}
		idx = n
	}

	row := m.lastResult.Data[idx-1]

	maxKey := 0
	for _, c := range m.lastResult.Columns {
		if l := len(c.Name); l > maxKey {
			maxKey = l
		}
	}

	m.appendStyled(fmt.Sprintf("Row %d", idx), hintStyle)

	for i, col := range m.lastResult.Columns {
		key := padRight(col.Name, maxKey)
		val := formatValue(row[i])

		line := tableHeaderStyle.Render(key) +
			"  " +
			tableBorderStyle.Render("›") +
			"  " +
			tableBodyStyle.Render(val)

		m.appendLines(line)
	}
}

// ---------- command history ----------

func (m *Model) pushCmd(line string) {
	if line == "" {
		return
	}
	n := len(m.cmdHistory)
	if n == 0 || m.cmdHistory[n-1] != line {
		m.cmdHistory = append(m.cmdHistory, line)
	}
	m.cmdIndex = len(m.cmdHistory)
}

func (m *Model) historyPrev() {
	if len(m.cmdHistory) == 0 {
		return
	}
	if m.cmdIndex > 0 {
		m.cmdIndex--
	}
	m.input.SetValue(m.cmdHistory[m.cmdIndex])
	m.input.CursorEnd()
}

func (m *Model) historyNext() {
	if len(m.cmdHistory) == 0 {
		return
	}

	if m.cmdIndex < len(m.cmdHistory)-1 {
		m.cmdIndex++
		m.input.SetValue(m.cmdHistory[m.cmdIndex])
		m.input.CursorEnd()
		return
	}

	m.cmdIndex = len(m.cmdHistory)
	m.input.SetValue("")
	m.input.CursorEnd()
}

// ---------- misc helpers ----------

func formatValue(v any) string {
	if v == nil {
		return "<null>"
	}
	switch t := v.(type) {
	case []byte:
		s := string(t)
		for _, r := range s {
			if r < 32 && r != '\n' && r != '\t' {
				return fmt.Sprintf("<blob %d bytes>", len(t))
			}
		}
		return s
	default:
		return fmt.Sprint(t)
	}
}

func Run(ctx context.Context, d db.DB) error {
	m := NewModel(ctx, d)
	p := tea.NewProgram(m, tea.WithAltScreen(), tea.WithMouseCellMotion())
	_, err := p.Run()
	return err
}

// padToWidth builds: left + spaces + right, clipped to width.
func padToWidth(left, right string, width int) string {
	if width <= 0 {
		if left == "" {
			return right
		}
		return left + "  " + right
	}

	minGap := 2
	maxLen := width
	if maxLen < len(left)+minGap {
		// can't fit right side at all
		if len(left) > width {
			return left[:width]
		}
		return left
	}

	// space left for right side
	spaceForRight := width - len(left) - minGap
	if spaceForRight <= 0 {
		if len(left) > width {
			return left[:width]
		}
		return left
	}

	if len(right) > spaceForRight {
		right = right[:spaceForRight]
	}

	gap := strings.Repeat(" ", width-len(left)-len(right))
	return left + gap + right
}

// boxed table rendering

func renderBoxTable(rows *db.Rows, maxColWidth int) []string {
	n := len(rows.Columns)
	if n == 0 {
		return []string{"(no columns)"}
	}
	if maxColWidth <= 0 {
		maxColWidth = 40
	}

	widths := make([]int, n)
	for i, col := range rows.Columns {
		w := len(col.Name)
		if w > maxColWidth {
			w = maxColWidth
		}
		widths[i] = w
	}

	for _, r := range rows.Data {
		for i, cell := range r {
			s := fmt.Sprint(cell)
			if len(s) > maxColWidth {
				s = s[:maxColWidth]
			}
			if len(s) > widths[i] {
				widths[i] = len(s)
			}
		}
	}

	names := make([]string, n)
	for i, c := range rows.Columns {
		names[i] = c.Name
	}

	var out []string
	out = append(out, buildBorder("┌", "┬", "┐", "─", widths))
	out = append(out, buildRow(names, widths, "│", "│"))
	out = append(out, buildBorder("├", "┼", "┤", "─", widths))
	for _, r := range rows.Data {
		cells := make([]string, n)
		for i, cell := range r {
			s := fmt.Sprint(cell)
			if len(s) > maxColWidth {
				s = s[:maxColWidth]
			}
			cells[i] = s
		}
		out = append(out, buildRow(cells, widths, "│", "│"))
	}
	out = append(out, buildBorder("└", "┴", "┘", "─", widths))

	return out
}

func buildBorder(left, mid, right, fill string, widths []int) string {
	var b strings.Builder
	b.WriteString(left)
	for i, w := range widths {
		b.WriteString(strings.Repeat(fill, w+2))
		if i < len(widths)-1 {
			b.WriteString(mid)
		}
	}
	b.WriteString(right)
	return b.String()
}

func buildRow(cells []string, widths []int, left, right string) string {
	var b strings.Builder
	b.WriteString(left)
	for i, cell := range cells {
		w := widths[i]
		if len(cell) > w {
			cell = cell[:w]
		}
		b.WriteString(" ")
		b.WriteString(padRight(cell, w))
		b.WriteString(" ")
		if i < len(widths)-1 {
			b.WriteString("│")
		}
	}
	b.WriteString(right)
	return b.String()
}

func padRight(s string, w int) string {
	if len(s) >= w {
		return s
	}
	return s + strings.Repeat(" ", w-len(s))
}

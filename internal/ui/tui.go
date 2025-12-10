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

// ANSI palette indices – actual colors come from the user's terminal theme.
const (
	colorFg      = "7" // default text
	colorMuted   = "8" // dim / comments
	colorAccent1 = "4" // title accent
	colorAccent2 = "6" // prompts / meta / header
	colorBorder  = "6" // table borders + body
	colorError   = "1" // errors
)

// Styles – foreground only, background is whatever the terminal uses.
var (
	baseStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color(colorFg))

	titleStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorAccent1)).
			Bold(true)

	dbBadgeStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorAccent2)).
			Bold(true)

	headerHelpStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorMuted))

	// general body text
	textStyle = baseStyle.Copy()

	// muted hints (footer legend etc.)
	hintStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorMuted))

	metaEchoStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorAccent2))

	errorStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorError))

	tableBorderStyle = baseStyle.Copy().
				Foreground(lipgloss.Color(colorBorder))

	tableHeaderStyle = baseStyle.Copy().
				Foreground(lipgloss.Color(colorAccent2)).
				Bold(true)

	// body cells same color as borders
	tableBodyStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorBorder))

	promptStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorAccent2)).
			Bold(true)

	inputTextStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorFg))

	footerStyle = baseStyle.Copy().
			Foreground(lipgloss.Color(colorMuted))

	// No global fg/bg; let terminal + individual styles handle it.
	rootStyle     = lipgloss.NewStyle()
	viewportStyle = lipgloss.NewStyle()
)

type Mode int

const (
	ModeQuery Mode = iota
	ModeHelp
)

type layout struct {
	headerLines   int
	promptLines   int
	contentHeight int
}

func computeLayout(width, height int) layout {
	const headerLines = 1
	const promptLines = 2 // prompt + footer

	used := headerLines + promptLines
	contentHeight := height - used
	if contentHeight < 3 {
		contentHeight = 3
	}

	return layout{
		headerLines:   headerLines,
		promptLines:   promptLines,
		contentHeight: contentHeight,
	}
}

// Model is the Bubble Tea TUI model.
type Model struct {
	ctx         context.Context
	db          db.DB
	mode        Mode
	driverLabel string

	history []string        // line-based output log (already styled)
	input   textinput.Model // prompt

	width  int
	height int

	lastResult *db.Rows

	cmdHistory []string
	cmdIndex   int

	vp viewport.Model // scrollable content area
}

func NewModel(ctx context.Context, d db.DB, driverLabel string) Model {
	if driverLabel == "" {
		driverLabel = "db"
	}

	ti := textinput.New()
	ti.Placeholder = ""
	ti.Prompt = fmt.Sprintf("BINSQL:%s> ", driverLabel)
	ti.PromptStyle = promptStyle
	ti.TextStyle = inputTextStyle
	ti.Focus()

	vp := viewport.New(0, 0)
	vp.Style = viewportStyle

	return Model{
		ctx:         ctx,
		db:          d,
		mode:        ModeQuery,
		driverLabel: driverLabel,
		history:     []string{},
		input:       ti,
		vp:          vp,
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
		// Help mode short-circuit
		if m.mode == ModeHelp {
			switch msg.String() {
			case "esc", "q":
				m.mode = ModeQuery
				m.refreshViewport()
				handled = true
			case "ctrl+c":
				return m, tea.Quit
			}
		}

		if handled {
			break
		}

		switch msg.String() {
		case "ctrl+c":
			return m, tea.Quit

		case "?":
			// Open help only when prompt is empty
			if strings.TrimSpace(m.input.Value()) == "" {
				m.mode = ModeHelp
				m.vp.SetContent(helpContent())
				m.vp.GotoTop()
				handled = true
			}

		// History: Ctrl+J / Ctrl+K (vim-style), always active
		case "ctrl+k":
			m.historyPrev()
			handled = true

		case "ctrl+j":
			m.historyNext()
			handled = true

		// Viewport scrolling: Ctrl+U / Ctrl+D
		case "ctrl+u":
			if m.vp.Height > 0 {
				m.vp.LineUp(m.vp.Height / 2)
			} else {
				m.vp.LineUp(5)
			}
			handled = true

		case "ctrl+d":
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

		l := computeLayout(msg.Width, msg.Height)

		m.vp.Width = msg.Width
		m.vp.Height = l.contentHeight
		m.refreshViewport()
	}

	var vpCmd tea.Cmd
	m.vp, vpCmd = m.vp.Update(msg)
	if vpCmd != nil {
		cmd = tea.Batch(cmd, vpCmd)
	}

	if !handled {
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

	headerLeft := titleStyle.Render("BINSQL") + " " +
		dbBadgeStyle.Render("["+m.driverLabel+"]")

	var headerRight string
	switch m.mode {
	case ModeHelp:
		headerRight = headerHelpStyle.Render("HELP · esc/q to close")
	default:
		headerRight = headerHelpStyle.Render("/dt tables  ·  /e [n] expand  ·  /q quit  ·  Ctrl+J/K history  ·  ? help")
	}

	header := padToWidth(headerLeft, headerRight, m.width)

	b.WriteString(header)
	b.WriteString("\n")

	b.WriteString(m.vp.View())
	b.WriteString("\n")

	b.WriteString(m.input.View())
	b.WriteString("\n")

	b.WriteString(
		footerStyle.Render("^C quit  ·  Ctrl+J/K history  ·  ? help  ·  Developed with <3 by @bgunnarsson"),
	)
	b.WriteString("\n")

	return rootStyle.Render(b.String())
}

// ---------- help content ----------

func helpContent() string {
	var lines []string

	// Title
	lines = append(lines, titleStyle.Render(" BINSQL HELP "))
	lines = append(lines, strings.Repeat("─", 40))

	// Meta commands
	sectionTitle := headerHelpStyle.Copy().Bold(true).Render(" Meta commands ")
	lines = append(lines, "")
	lines = append(lines, sectionTitle)
	lines = append(lines, helpRow(`/dt`, "List tables"))
	lines = append(lines, helpRow(`/e N`, "Expand row N from last result"))
	lines = append(lines, helpRow(`/q`, "Quit binsql"))

	// Keys
	sectionTitle = headerHelpStyle.Copy().Bold(true).Render(" Keys ")
	lines = append(lines, "")
	lines = append(lines, sectionTitle)
	lines = append(lines, helpRow("Ctrl+J / Ctrl+K", "Command history"))
	lines = append(lines, helpRow("Ctrl+U / Ctrl+D", "Scroll output"))
	lines = append(lines, helpRow("enter", "Execute SQL/meta command"))
	lines = append(lines, helpRow("?", "Show this help (when prompt is empty)"))
	lines = append(lines, helpRow("esc / q", "Close help"))
	lines = append(lines, helpRow("ctrl+c", "Quit immediately"))

	// Tips
	sectionTitle = headerHelpStyle.Copy().Bold(true).Render(" Tips ")
	lines = append(lines, "")
	lines = append(lines, sectionTitle)
	lines = append(lines, textStyle.Render("• Use /e 1, /e 2, ... to inspect wide rows"))
	lines = append(lines, textStyle.Render("• Most SQL works as-is; meta commands always start with /"))
	lines = append(lines, textStyle.Render("• History keeps recent commands – use Ctrl+J / Ctrl+K"))
	lines = append(lines, textStyle.Render("• Scroll output with Ctrl+U / Ctrl+D"))

	return strings.Join(lines, "\n")
}

// Render one help row: aligned command + description.
func helpRow(cmd, desc string) string {
	const cmdColWidth = 18

	rawCmd := padRight(cmd, cmdColWidth)
	styledCmd := promptStyle.Render(rawCmd)
	styledDesc := textStyle.Render(desc)

	return "  " + styledCmd + "  " + styledDesc
}

// ---------- history + viewport helpers ----------

func (m *Model) refreshViewport() {
	m.vp.SetContent(strings.Join(m.history, "\n"))
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

	if strings.HasPrefix(line, "/") {
		return m.runMeta(line)
	}

	m.runSQL(line)
	return nil
}

func (m *Model) runMeta(cmd string) tea.Cmd {
	s := strings.TrimSpace(cmd)

	switch {
	case s == `/q`:
		m.appendStyled("Bye.", textStyle)
		return tea.Quit

	case s == `/dt`:
		m.listTables()
		return nil

	case strings.HasPrefix(s, `/e`):
		m.expandRow(s)
		return nil

	default:
		m.appendStyled("Unknown command: "+cmd, errorStyle)
		return nil
	}
}

// /dt – boxed table of relations
func (m *Model) listTables() {
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
		names, err2 := m.db.ListTables(m.ctx)
		if err2 != nil {
			m.appendStyled("error listing tables: "+err2.Error(), errorStyle)
			return
		}
		if len(names) == 0 {
			m.appendLines("(no relations)")
			return
		}

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

	m.appendStyled("List of relations", textStyle)
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
	m.appendStyled(fmt.Sprintf("(%d rows)", len(rows.Data)), textStyle)
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
		m.appendStyled(`Use /e [rowNumber] to expand a row (example: /e 1).`, textStyle)
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
		case 0, 2, len(lines)-1:
			m.appendLines(tableBorderStyle.Render(line))
		default:
			m.appendLines(tableBodyStyle.Render(line))
		}
	}

	m.appendStyled(fmt.Sprintf("(%d rows)", len(rows.Data)), textStyle)
}

// /e / /e 3 – expand row
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
			m.appendStyled("usage: /e [rowNumber]", errorStyle)
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

	m.appendStyled(fmt.Sprintf("Row %d", idx), textStyle)

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

func Run(ctx context.Context, d db.DB, driverLabel string) error {
	m := NewModel(ctx, d, driverLabel)
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
		if len(left) > width {
			return left[:width]
		}
		return left
	}

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

// boxed table rendering (text-based, but clean)
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

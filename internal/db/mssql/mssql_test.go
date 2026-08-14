package mssql

import "testing"

// SQL Server has no LIMIT clause, so row limiting must go through TOP.
func TestSelectLimit(t *testing.T) {
	tests := []struct {
		name  string
		table string
		where string
		n     int
		want  string
	}{
		{
			name:  "limit only",
			table: "[dbo].[users]",
			n:     10,
			want:  "SELECT TOP (10) * FROM [dbo].[users]",
		},
		{
			name:  "limit with predicate",
			table: "[dbo].[orders]",
			where: "status = 'open'",
			n:     5,
			want:  "SELECT TOP (5) * FROM [dbo].[orders] WHERE status = 'open'",
		},
		{
			name:  "no limit",
			table: "[dbo].[users]",
			want:  "SELECT * FROM [dbo].[users]",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := selectLimit(tt.table, tt.where, tt.n); got != tt.want {
				t.Errorf("selectLimit() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestQuoteIdent(t *testing.T) {
	tests := map[string]string{
		"users":    "[users]",
		"my table": "[my table]",
		"we[i]rd":  "[we[i]]rd]",
		"order":    "[order]",
	}
	for in, want := range tests {
		if got := quoteIdent(in); got != want {
			t.Errorf("quoteIdent(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestFormatUniqueIdentifier(t *testing.T) {
	// SQL Server stores the first three groups little-endian.
	b := []byte{
		0x78, 0x56, 0x34, 0x12,
		0x34, 0x12,
		0x56, 0x34,
		0x78, 0x9a,
		0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56,
	}
	want := "12345678-1234-3456-789a-bcdef0123456"
	if got := formatUniqueIdentifier(b); got != want {
		t.Errorf("formatUniqueIdentifier() = %q, want %q", got, want)
	}

	// A wrong-length value must not panic.
	if got := formatUniqueIdentifier([]byte{0x01, 0x02}); got != "0102" {
		t.Errorf("short input = %q", got)
	}
}

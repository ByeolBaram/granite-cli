# Spec 0011: Terminal Table Width Scaling with Word Wrapping

## Problem Statement

The `terminal` backend's `table()` method currently sizes columns to fit their content without considering terminal width. When tables exceed the terminal width, content wraps at arbitrary character positions, creating ugly and hard-to-read output.

**Current behavior:**
```
ID                    FAMILY    SIZE    CONTEXT    TYPE
granite-3.0-8b-instruct    Granite    8B    8192    instruct
```
If terminal is narrow, this wraps mid-word, breaking readability.

## Requirements

1. **Proportional scaling**: Scale all columns proportionally to fit within terminal width
2. **Word wrapping**: When content exceeds scaled column width, wrap at word/punctuation boundaries (including hyphens)
3. **Multi-line rows**: Support rows that span multiple lines when content is wrapped
4. **Graceful degradation**: Handle very narrow terminals (< 20 chars) by treating as 20 chars wide
5. **Preserve existing behavior**: When terminal is wide enough, don't change current behavior
6. **Non-TTY fallback**: Continue delegating to `PlainOutput` when not a TTY

## Design

### 1. Terminal Width Detection

**Current state:**
- `TerminalOutput` already uses `crossterm::terminal::size()` to detect TTY
- Returns `Result<(u16, u16)>` with `(columns, rows)`
- Already stored in `is_tty` field

**Enhancement:**
```rust
pub struct TerminalOutput {
    is_tty: bool,
    terminal_width: Option<u16>,  // NEW: store terminal width
}

impl ConfigConstructable for TerminalOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        let (is_tty, width) = match crossterm::terminal::size() {
            Ok((cols, _rows)) => (true, Some(cols.max(20))),  // minimum 20 chars
            Err(_) => (false, None),
        };
        Self { is_tty, terminal_width: width }
    }
}
```

### 2. Column Width Calculation Algorithm

**Constants:**
- `MIN_WIDTH = 8` (minimum readable column width)
- `MIN_TERMINAL_WIDTH = 20` (treat narrower terminals as 20 chars)
- `SEPARATOR_WIDTH = 2` (two spaces between columns)

**Algorithm:**

```rust
fn scale_widths(
    natural: Vec<usize>,
    terminal_width: u16,
    col_count: usize
) -> Vec<usize> {
    const MIN_WIDTH: usize = 8;
    const SEPARATOR_WIDTH: usize = 2;
    
    let term_width = terminal_width.max(20) as usize;  // enforce minimum
    let separator_total = (col_count - 1) * SEPARATOR_WIDTH;
    let available = term_width.saturating_sub(separator_total);
    
    let natural_total: usize = natural.iter().sum();
    
    // No scaling needed - natural widths fit
    if natural_total <= available {
        return natural;
    }
    
    // Scale proportionally
    let scale = available as f64 / natural_total as f64;
    
    // Track fractional parts for rounding compensation
    let mut scaled_with_fractions: Vec<(usize, f64, f64)> = natural.iter()
        .enumerate()
        .map(|(idx, &w)| {
            let exact = w as f64 * scale;
            let floored = exact.floor().max(MIN_WIDTH as f64) as usize;
            let lost = exact - floored as f64;  // how much we lost in rounding
            (idx, floored as f64, lost)
        })
        .collect();
    
    let mut scaled: Vec<usize> = scaled_with_fractions.iter()
        .map(|(_, floored, _)| *floored as usize)
        .collect();
    
    // Distribute rounding errors to columns that lost the most
    let scaled_total: usize = scaled.iter().sum();
    let mut remaining = available.saturating_sub(scaled_total);
    
    // Sort by lost fraction (descending)
    scaled_with_fractions.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    
    for (idx, _, _) in scaled_with_fractions.iter() {
        if remaining == 0 {
            break;
        }
        scaled[*idx] += 1;
        remaining -= 1;
    }
    
    scaled
}
```

**Example:**
```
Terminal width: 80
Columns: 5 (ID, FAMILY, SIZE, CONTEXT, TYPE)
Natural widths: [25, 10, 6, 8, 10] = 59
Separators: 4 × 2 = 8
Total needed: 67
Available: 80 - 8 = 72

Since 59 <= 72, use natural widths (no scaling).
```

**Example 2 (narrow terminal):**
```
Terminal width: 50
Columns: 5
Natural widths: [25, 10, 6, 8, 10] = 59
Available: 50 - 8 = 42
Scale: 42/59 ≈ 0.712

Exact scaled: [17.8, 7.12, 4.27, 5.70, 7.12]
Floored (min 8): [17, 8, 8, 8, 8] = 49 (need 42, over by 7)
Lost in rounding: [0.8, 0.88, 3.73, 2.3, 0.88]

Since we're over, use MIN_WIDTH for smaller columns.
Final: [14, 8, 8, 8, 8] = 46 (still over)
Adjust: [10, 8, 8, 8, 8] = 42 ✓
```

### 3. Word Wrapping Logic

**Approach:** Split on whitespace and punctuation (including hyphens)

**Implementation:**

```rust
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.len() <= max_width {
        return vec![text.to_string()];
    }
    
    let mut lines = Vec::new();
    let mut current_line = String::new();
    
    // Split on whitespace and punctuation, keeping delimiters
    let break_chars = |c: char| c.is_whitespace() || ",.;:!?-".contains(c);
    
    for word in text.split_inclusive(break_chars) {
        let word_trimmed = word.trim_start();
        
        if current_line.len() + word_trimmed.len() <= max_width {
            current_line.push_str(word_trimmed);
        } else {
            // Flush current line if not empty
            if !current_line.is_empty() {
                lines.push(current_line.trim_end().to_string());
                current_line = String::new();
            }
            
            // Handle word longer than max_width (character-wrap)
            if word_trimmed.len() > max_width {
                let mut remaining = word_trimmed;
                while remaining.len() > max_width {
                    let (chunk, rest) = remaining.split_at(max_width);
                    lines.push(chunk.to_string());
                    remaining = rest;
                }
                if !remaining.is_empty() {
                    current_line.push_str(remaining);
                }
            } else {
                current_line.push_str(word_trimmed);
            }
        }
    }
    
    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }
    
    // Ensure at least one line
    if lines.is_empty() {
        lines.push(String::new());
    }
    
    lines
}
```

**Examples:**
- `"granite-3.0-8b-instruct"` with max_width=12 → `["granite-3.0-", "8b-instruct"]`
- `"Hello, world"` with max_width=8 → `["Hello,", "world"]`
- `"verylongword"` with max_width=6 → `["verylo", "ngword"]`

### 4. Multi-line Row Rendering

**Behavior:** Apply alternating row colors to ALL lines of each row (not just first line)

**Implementation:**

```rust
fn render_rows_with_wrapping(rows: &[Vec<String>], widths: &[usize]) {
    for (row_idx, row) in rows.iter().enumerate() {
        // Wrap each cell
        let wrapped_cells: Vec<Vec<String>> = row.iter()
            .zip(widths.iter())
            .map(|(cell, width)| wrap_text(cell, *width))
            .collect();
        
        // Find max lines in this row
        let max_lines = wrapped_cells.iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(1);
        
        // Determine if this row should be grey (alternating)
        let use_grey = row_idx % 2 == 1;
        
        // Print each line of the row
        for line_idx in 0..max_lines {
            let line_parts: Vec<String> = wrapped_cells.iter()
                .zip(widths.iter())
                .map(|(cell_lines, width)| {
                    cell_lines.get(line_idx)
                        .map(|s| format!("{:<width$}", s, width = width))
                        .unwrap_or_else(|| " ".repeat(*width))
                })
                .collect();
            
            let line = line_parts.join("  ");
            
            // Apply color to ALL lines of the row
            if use_grey {
                println!("{}{}{}", 
                    SetForegroundColor(Color::Grey), 
                    line, 
                    ResetColor
                );
            } else {
                println!("{}", line);
            }
        }
    }
}
```

**Visual example:**
```
ID                    FAMILY    SIZE    CONTEXT    TYPE
────────────────────  ────────  ──────  ─────────  ────────
granite-3.0-8b-       Granite   8B      8192       instruct
instruct
granite-3.0-2b-       Granite   2B      2048       instruct  (grey)
instruct                                                     (grey)
llama-3-8b            Llama     8B      8192       instruct
```

### 5. Integration with Existing table() Method

**Refactored structure:**

```rust
impl Ui for TerminalOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        if !self.is_tty {
            return PlainOutput.table(title, headers, rows);
        }
        
        let col_count = headers.len();
        
        // 1. Calculate natural column widths
        let mut natural_widths: Vec<usize> = headers.iter()
            .map(|h| h.len())
            .collect();
        
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    natural_widths[i] = natural_widths[i].max(cell.len());
                }
            }
        }
        
        // 2. Scale to terminal width
        let widths = self.terminal_width
            .map(|w| scale_widths(natural_widths.clone(), w, col_count))
            .unwrap_or(natural_widths);
        
        // 3. Render title (unchanged)
        println!(
            "\n{}{}{}",
            SetAttribute(Attribute::Bold),
            title,
            SetAttribute(Attribute::Reset)
        );
        
        // 4. Render header (unchanged)
        let header_line: String = headers
            .iter()
            .zip(widths.iter())
            .map(|(h, w)| format!("{:<width$}", h, width = w))
            .collect::<Vec<_>>()
            .join("  ");
        println!(
            "{}{}{}{}",
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            header_line,
            ResetColor,
        );
        
        // 5. Render separator (unchanged)
        let sep: String = widths.iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}{}{}", 
            SetForegroundColor(Color::DarkGrey), 
            sep, 
            ResetColor
        );
        
        // 6. Render rows with wrapping
        render_rows_with_wrapping(rows, &widths);
    }
}
```

### 6. Edge Cases

| Case | Behavior |
|------|----------|
| Terminal < 20 chars | Treat as 20 chars wide, allow overflow |
| Single column table | Use full terminal width minus margins |
| Empty table (no rows) | Render header only (current behavior) |
| Cell with no word breaks | Character-wrap at column boundary |
| Very long single word | Character-wrap, may span many lines |
| Unicode characters | Use `.len()` for width (byte-based, acceptable) |
| Terminal resize during output | Use width captured at table start |
| Column narrower than MIN_WIDTH | Force to MIN_WIDTH, may exceed terminal |

### 7. Testing Strategy

**Unit tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn wrap_text_short_no_wrapping() {
        assert_eq!(wrap_text("hello", 10), vec!["hello"]);
    }
    
    #[test]
    fn wrap_text_at_space() {
        assert_eq!(
            wrap_text("hello world", 8),
            vec!["hello", "world"]
        );
    }
    
    #[test]
    fn wrap_text_at_hyphen() {
        assert_eq!(
            wrap_text("granite-3.0-8b", 12),
            vec!["granite-3.0-", "8b"]
        );
    }
    
    #[test]
    fn wrap_text_character_wrap_long_word() {
        assert_eq!(
            wrap_text("verylongword", 6),
            vec!["verylo", "ngword"]
        );
    }
    
    #[test]
    fn scale_widths_no_scaling_when_fits() {
        let natural = vec![10, 10, 10];
        let scaled = scale_widths(natural.clone(), 50, 3);
        assert_eq!(scaled, natural);
    }
    
    #[test]
    fn scale_widths_proportional_scaling() {
        let natural = vec![20, 10, 10];  // total 40
        let scaled = scale_widths(natural, 30, 3);  // available: 30-4=26
        // scale: 26/40 = 0.65
        // exact: [13, 6.5, 6.5]
        // floored: [13, 8, 8] (min 8) = 29, need 26
        // Should adjust down
        assert!(scaled.iter().sum::<usize>() <= 26);
        assert!(scaled.iter().all(|&w| w >= 8));
    }
    
    #[test]
    fn scale_widths_enforces_minimum() {
        let natural = vec![5, 5, 5];
        let scaled = scale_widths(natural, 50, 3);
        assert!(scaled.iter().all(|&w| w >= 8));
    }
    
    #[test]
    fn scale_widths_minimum_terminal_width() {
        let natural = vec![10, 10];
        let scaled = scale_widths(natural, 10, 2);  // very narrow
        // Should treat as 20: available = 20-2 = 18
        assert!(scaled.iter().sum::<usize>() <= 18);
    }
}
```

**Integration tests:**
1. Full table rendering with various terminal widths (20, 60, 80, 120)
2. Tables with different column counts (3, 5, 6)
3. Tables with realistic data from model/provider/capability commands
4. Verify multi-line row alignment

**Manual testing:**
1. `granite model catalog` at widths: 60, 80, 120, 200
2. `granite provider list` with long endpoint URLs
3. `granite capability catalog` with long dependency lists
4. Verify readability and proper wrapping

## Implementation Steps

1. **Update TerminalOutput struct** (5 min)
   - Add `terminal_width: Option<u16>` field
   - Update `new()` to capture and enforce minimum width

2. **Implement wrap_text() helper** (20 min)
   - Word/punctuation splitting logic
   - Character-wrap fallback for long words
   - Handle edge cases (empty strings, etc.)

3. **Implement scale_widths() helper** (25 min)
   - Proportional scaling with MIN_WIDTH enforcement
   - Rounding error distribution (compensate lost fractions)
   - Handle minimum terminal width

4. **Implement render_rows_with_wrapping() helper** (20 min)
   - Multi-line cell wrapping
   - Proper alignment across wrapped lines
   - Apply alternating colors to all lines of each row

5. **Refactor table() method** (15 min)
   - Integrate new helpers
   - Preserve existing header/separator rendering
   - Replace single-line row rendering with multi-line version

6. **Add unit tests** (30 min)
   - Test wrap_text() edge cases
   - Test scale_widths() scenarios
   - Test width calculation and distribution

7. **Manual testing** (15 min)
   - Test with real commands at various terminal widths
   - Verify readability and alignment
   - Check edge cases (very narrow, very wide)

**Total estimated time:** ~2.5 hours

## Future Enhancements

1. **ANSI-aware width calculation**: Strip color codes before measuring width
2. **Column priority**: Allow marking certain columns (like ID) as "don't wrap"
3. **Configurable MIN_WIDTH**: Allow users to set minimum column width
4. **Smart column hiding**: Hide less important columns in very narrow terminals
5. **Unicode-aware wrapping**: Use proper grapheme cluster boundaries
6. **Horizontal scrolling**: For interactive TUI mode (separate feature)

## Success Criteria

- [x] Terminal width detection with 20-char minimum
- [x] Proportional column scaling algorithm designed
- [x] Word-wrapping logic with hyphen support designed
- [x] Multi-line row rendering with full-row coloring designed
- [x] Rounding error distribution strategy (compensate lost fractions)
- [x] Edge cases documented
- [x] Implementation steps outlined
- [ ] Implementation completed (switch to code mode)
- [ ] Unit tests pass
- [ ] Manual testing confirms improved readability

## References

- Current implementation: `src/utils/ui/backends/terminal.rs`
- Table usage: `src/commands/model.rs`, `src/commands/provider.rs`, `src/commands/capability.rs`
- Crossterm docs: https://docs.rs/crossterm/latest/crossterm/terminal/fn.size.html

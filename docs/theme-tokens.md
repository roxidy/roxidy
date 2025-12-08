# Theme Token Reference

This document maps hardcoded colors to their theme token equivalents for consistent theming.

## CSS Variables Available

### UI Colors
- `--background` - Main background
- `--foreground` - Primary text
- `--card` - Card/elevated surface background
- `--card-foreground` - Text on cards
- `--muted` - Muted background
- `--muted-foreground` - Muted text (secondary text)
- `--primary` - Primary brand color
- `--primary-foreground` - Text on primary
- `--accent` - Accent background
- `--accent-foreground` - Text on accent
- `--destructive` - Error/destructive color
- `--border` - Border color
- `--input` - Input border
- `--ring` - Focus ring

### ANSI Colors (for semantic color usage)
- `--ansi-blue` - Information, user actions
- `--ansi-green` - Success states
- `--ansi-yellow` - Warning states
- `--ansi-red` - Error states
- `--ansi-magenta` - Agent/AI actions
- `--ansi-cyan` - Thinking/processing
- `--ansi-white` - Standard text
- `--ansi-bright-*` - Brighter variants
- `--ansi-default-fg` - Terminal foreground
- `--ansi-default-bg` - Terminal background

## Tailwind Utility Classes

### Predefined Classes (use these when available)
- `text-foreground` / `bg-foreground`
- `text-muted-foreground` / `bg-muted`
- `text-primary` / `bg-primary`
- `text-destructive` / `bg-destructive`
- `border-border`
- `bg-card` / `text-card-foreground`
- `bg-accent` / `text-accent-foreground`

### Custom Classes (for ANSI colors)
Use `text-[var(--ansi-blue)]` or `bg-[var(--ansi-blue)]/20` for opacity

## Migration Mapping

| Old Hardcoded Color | Semantic Use | New Token | Tailwind Class |
|---------------------|--------------|-----------|----------------|
| `#7aa2f7` | Blue - User/Info | `--ansi-blue` | `text-[var(--ansi-blue)]` |
| `#bb9af7` | Purple - Agent/AI | `--ansi-magenta` | `text-[var(--ansi-magenta)]` |
| `#e0af68` | Yellow - Warning | `--ansi-yellow` | `text-[var(--ansi-yellow)]` |
| `#9ece6a` | Green - Success | `--ansi-green` | `text-[var(--ansi-green)]` |
| `#f7768e` | Red - Error | `--ansi-red` | `text-[var(--ansi-red)]` |
| `#7dcfff` | Cyan - Thinking | `--ansi-cyan` | `text-[var(--ansi-cyan)]` |
| `#c0caf5` | Primary text | `--foreground` | `text-foreground` |
| `#565f89` | Muted/secondary | `--muted-foreground` | `text-muted-foreground` |
| `#787c99` | Slightly lighter muted | `--muted-foreground` | `text-muted-foreground` |
| `#1f2335` | Card background | `--card` | `bg-card` |
| `#16161e` | Darker card | `--muted` | `bg-muted` |
| `#13131a` | Darkest bg | `--background` | `bg-background` |
| `#1a1b26` | Hover state | `--accent` | `bg-accent` |
| `#27293d` | Border | `--border` | `border-border` |

## Pattern Examples

### Status Badges
```tsx
// Old
className="bg-[#9ece6a]/20 text-[#9ece6a]"

// New
className="bg-[var(--ansi-green)]/20 text-[var(--ansi-green)]"
```

### Message Styling
```tsx
// Old
className={isUser ? "bg-[#7aa2f7]/20" : "bg-[#bb9af7]/20"}

// New
className={isUser ? "bg-[var(--ansi-blue)]/20" : "bg-[var(--ansi-magenta)]/20"}
```

### Text Colors
```tsx
// Old
className="text-[#c0caf5]"

// New
className="text-foreground"
```

### Borders
```tsx
// Old
className="border-l-[#9ece6a]"

// New
className="border-l-[var(--ansi-green)]"
```

## Notes
- Use semantic ANSI colors for status-based coloring (success, error, warning, info)
- Use UI tokens (foreground, muted-foreground) for general text
- Opacity modifiers work with CSS variables: `bg-[var(--ansi-blue)]/20`
- Some components (like MockDevTools) may intentionally use hardcoded colors if they're meant to be theme-independent

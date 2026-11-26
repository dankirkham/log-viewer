- [x] Removed the redundant heatmap range legend labels beneath the VE and AFR
  tables; operating-point diagnostics remain visible when relevant.
- [x] Gauge cards are placed in explicit width-aware rows. Each card reports
  its full measured size so later rows start below the tallest card; a UI
  regression test checks wrapping and prevents horizontal overlap.
- [x] The AFR scatter plot uses the same reversed color function as the
  AFR table, with AFR 14.7 anchored in blue.

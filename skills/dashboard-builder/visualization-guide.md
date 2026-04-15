# Visualization Selection Guide

## When to Use Each Chart Type

| Data Question | Recommended Viz | Why |
|--------------|----------------|-----|
| How does a metric change over time? | Line chart | Shows trends and seasonality clearly |
| What is the current value of a KPI? | Metric tile | Single number with optional comparison |
| How do categories compare? | Horizontal bar chart | Easy to read labels, good for many categories |
| What is the distribution? | Histogram | Shows data shape and outliers |
| What proportion does each part contribute? | Donut chart | Good for 2-7 segments, avoid for more |
| Where are events happening? | Map (coordinate or region) | Geographic context at a glance |
| What are the raw details? | Data table | When users need to drill into specifics |
| How do two metrics correlate? | Scatter plot | Reveals relationships between variables |
| What is the status of multiple items? | Heat map | Dense overview of many categories over time |

## Best Practices

### Color Usage
- Use a consistent color palette across the dashboard
- Reserve red for critical/error states
- Use green sparingly (only for confirmed-good states)
- Ensure sufficient contrast for accessibility

### Data Density
- Metric tiles: 1 number each, max 6-8 per row
- Time series: max 5-7 series per chart before it gets noisy
- Tables: limit to 10-15 columns, paginate rows
- Donut/pie: max 7 segments, group the rest as "Other"

### Performance
- Avoid `*` field selections; specify only needed fields
- Use date histograms with appropriate intervals (auto is usually fine)
- Add index pattern filters to reduce scan scope
- Consider using transforms for pre-aggregated heavy dashboards

### Layout Tips
- Most important information goes top-left (natural reading order)
- Group related visualizations together
- Use consistent panel sizing within rows
- Add markdown panels for section headers or explanatory notes

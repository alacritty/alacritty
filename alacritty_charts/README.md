# Alacritty Charts
This modules stores data in a time series circular buffer of items like `(u64, Option<f64>)`
The tuple consists of an epoch (u64) and an optional value at that time.

The circular buffer is used to avoid memory relocation on growth.

## Data Sources
The source of the data for the metrics can be internal metrics (such as the
amount of keystroke inputs or written lines through times) or they can be
loaded from Prometheus servers. Other data sources can be configured.

## Example configuration
The
```yaml
- name: async loaded items
  offset:
    x: 780
  width: 100
  height: 25
  series:
  - name: Number of input items for TimeSeries
    type: async_items_loaded
    refresh: 1
    color: "0x00ff00"
    alpha: 1.0
```

# Alacritty Charts

###### tags: `Tag(Prometheus)` `Tag(TimeSeries)` `Tag(Tokio)`

[ToC]

## tl;dr
Asynchronously load time series data to draw charts in alacritty

## Disclaimer
This was an idea of a project to learn Rust and programming. Many things are far from ideal/decent and could/should be replaced by existing efficient crates/patterns.

## Design
This modules stores metric data in a time series circular buffer of items of type `(u64, Option<f64>)`
The tuple consists of an epoch (u64) and an optional value at that time.

A circular buffer technique is used to avoid memory relocation on growth.

The charts data is owned by an `async_coordinator` tokio spawned task to avoid slowing down the main thread when
data has to be loaded from remote sources or when calculations have to be performed on the metrics
data.

## Data Sources
The source of the data for the metrics can be internal metrics (such as the
amount of keystroke inputs or written lines through time) or they can be
loaded from Prometheus servers. Other data sources may be available in the future.

## Example configuration
The `charts` should be part of the `alacritty.yml` config file. it does **NOT** support live reloading.
This is an example configuration with different data sources and comments. 

```yaml
charts:
- name: async loaded items   # A friendly name for logs
  offset:
    x: 980                   # pixels from the left
  width: 100
  height: 25                 # The max value will hit this height
  series:
  - name: Number of input items for TimeSeries
    type: async_items_loaded # Internal counter
    color: "0x00ff00"
    alpha: 1.0
- name: output newlines
    offset:
      x: 1090
    width: 100
    height: 25
    series:
    - name: output
      type: alacritty_output # Another internal counter
      color: "0x0000ff"
      alpha: 1.0
      # In case values are missing, fill them  with the
      # last successfully loaded value
      missing_values_policy: last
      # When the metric has already been filled, it should
      # Increment the value instead of overwriting it.
      collision_policy: Increment
- name: input newlines
  offset:
    x: 1100
  width: 100
  height: 25
  series:
  - name: input
    type: alacritty_input
    color: "0xff0000"
    alpha: 1.0
    missing_values_policy: last
    collision_policy: Increment
- name: load
  offset:
    x: 1210
  width: 100
  height: 25
  decorations:
  # Create a fixed line for reference at 4.0, this
  # makes sense on this Mac with 4 processors.
  - type: reference
    value: 4.0
    color: "0x03dac6"
    alpha: 0.3
    # The reference point contains whiskers like ggplot (R)
    height_multiplier: 0.05
  series:
  # This chart contains several series drawn in the same
  # space, so their statistics (max,min,etc) are calculated
  # together
  - name: load average 1 min
    # This is a metric loaded from a prometheus endpoint
    type: prometheus
    # Every 15 seconds, a call should be made to the endpoint
    # to pull new data
    refresh: 15
    source: 'http://localhost:9090/api/v1/query_range?query=node_load1'
    color: "0xbb86cf"
    alpha: 0.9
    missing_values_policy: avg
    # Because the source of truth is Prometheus,
    # whenever data arrives for an existing epoch,
    # we should just overwrite it.
    collision_policy: Overwrite
  - name: load average 5 min
    type: prometheus
    refresh: 15
    source: 'http://localhost:9090/api/v1/query_range?query=node_load5'
    color: "0xba68c8"
    alpha: 0.6
    missing_values_policy: avg
    collision_policy: Overwrite
  - name: load average 15 min
    type: prometheus
    refresh: 15
    source: 'http://localhost:9090/api/v1/query_range?query=node_load15'
    color: "0xee98fb"
    alpha: 0.3
    missing_values_policy: avg
    collision_policy: Overwrite
- name: memory
  offset:
    x: 1340
  width: 75
  height: 25
  decorations:
  - type: reference
    value: 1.0
    color: "0xffffff"
    alpha: 0.1
    height_multiplier: 0.05
  series:
  - name: memory total
    type: prometheus
    refresh: 15
    source: 'http://localhost:9090/api/v1/query_range?query=node_memory_bytes_total'
    color: "0xcf6679"
    alpha: 1.0
    missing_values_policy: avg
    collision_policy: Overwrite
  - name: memory used
    type: prometheus
    refresh: 15
    source: 'http://localhost:9090/api/v1/query_range?query=node_memory_active_bytes_total'
    color: "0xffffff"
    alpha: 1.0
    missing_values_policy: avg
    collision_policy: Overwrite
# Kubernetes clusters
- name: dev-cluster
  offset:
    x: 1450
  width: 75
  height: 25
  decorations:
  # Draw a reference point at 13 to identify clusters
  # with excessive nodes
  - type: reference
    value: 13.0
    color: "0x03dac6"
    alpha: 0.3
    height_multiplier: 0.05
  series:
  # When using cluster autoscaler, the following metric
  # can be exposed
  - name: Ready Nodes
    type: prometheus
    refresh: 15
    source: 'https://dev-cluster.internal.my-domain.com/prometheus/api/v1/query_range?query=cluster_autoscaler_nodes_count{state="ready"}'
    color: "0xbb86fc"
    alpha: 0.9
    missing_values_policy: avg
    collision_policy: Overwrite
```
Results in the following image
![example config](https://i.imgur.com/L6ba77U.png)


## Why Prometheus
Initially the data was loaded using proc_info crate but several drawbacks:
- Works on Linux, but not on OSX.
- Every terminal is pulling and drawing the state in real time and they are not synchronized.
- The calculations of the metric vertices were done in the main thread slowing down the terminal.
 
Using [Prometheus](https://prometheus.io/) on the other hand carried several advantages:
- The data is stored in an efficient format and the terminals query it so new terminals do not start with a blank state
- Monitoring remote resources is possible so long as they have prometheus running
- [Node Exporter](https://github.com/prometheus/node_exporter) collects many types of statistics, CPU, Disk usage, memory, without us having to find a way to monitor these metrics locally.


### Installing Prometheus and Node Exporter
#### OSX
[Node Exporter Brew](https://formulae.brew.sh/formula/node_exporter)
[Prometheus Brew](https://formulae.brew.sh/formula/prometheus)
```shell
brew install node_exporter
brew install prometheus
```

#### Arch Linux
```
pacman -S prometheus node_exporter
```

### Configuring Prometheus and Node Exporter

Prometheus needs to know that it should scrape(pull) metrics from Node Exporter, here's a basic configuration that ties them together in `$HOME/prometheus.yml`
```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s
alerting:
  alertmanagers:
  - static_configs:
    - targets:
    # - alertmanager:9093
rule_files:
# - "first_rules.yml"
# - "second_rules.yml"
scrape_configs:
- job_name: 'prometheus'
  static_configs:
  - targets: ['localhost:9090']
- job_name: 'node_exporter'
  static_configs:
  - targets: ['localhost:9100']
```

### Running Prometheus and Node Exporter
These should be started by their service units.
If you want to start prometheus and node_exporter manually, run:
```shell
$ node_exporter &
$ prometheus --log.level=warn --config.file ~/prometheus.yml &
```

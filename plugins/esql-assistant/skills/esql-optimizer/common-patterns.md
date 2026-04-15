# Common ES|QL Optimization Patterns

## Pattern: Top-N with High Cardinality

**Before (slow):**
```esql
FROM logs-*
| STATS count = COUNT(*) BY host.name
| SORT count DESC
```

**After (faster):**
```esql
FROM logs-*
| WHERE @timestamp >= NOW() - 1h
| STATS count = COUNT(*) BY host.name
| SORT count DESC
| LIMIT 20
```

**Why:** Adding a time bound and LIMIT prevents scanning the entire index and materializing all groups.

## Pattern: Filter Before Aggregate

**Before:**
```esql
FROM logs-*
| STATS error_count = COUNT(*) BY service.name
| WHERE error_count > 100
```

**After:**
```esql
FROM logs-*
| WHERE log.level == "error"
| STATS error_count = COUNT(*) BY service.name
| WHERE error_count > 100
```

**Why:** Filtering rows before aggregation reduces the number of documents Elasticsearch processes.

## Pattern: Project Early

**Before:**
```esql
FROM logs-*
| WHERE @timestamp >= NOW() - 24h
| SORT @timestamp DESC
| LIMIT 100
```

**After:**
```esql
FROM logs-*
| WHERE @timestamp >= NOW() - 24h
| KEEP @timestamp, message, host.name, log.level
| SORT @timestamp DESC
| LIMIT 100
```

**Why:** `KEEP` reduces the fields carried through the pipeline, lowering memory usage.

## Pattern: Null-Safe Aggregation

**Before:**
```esql
FROM metrics-*
| STATS avg_cpu = AVG(system.cpu.total.pct) BY host.name
```

**After:**
```esql
FROM metrics-*
| WHERE system.cpu.total.pct IS NOT NULL
| STATS avg_cpu = AVG(system.cpu.total.pct) BY host.name
```

**Why:** Null values can skew results or cause unexpected behavior in some aggregation contexts.

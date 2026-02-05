package io.streamhouse.client.model;

import java.util.List;

public class SqlResult {
    private List<ColumnInfo> columns;
    private List<List<Object>> rows;
    private int rowCount;
    private long executionTimeMs;
    private boolean truncated;

    public List<ColumnInfo> getColumns() { return columns; }
    public void setColumns(List<ColumnInfo> columns) { this.columns = columns; }

    public List<List<Object>> getRows() { return rows; }
    public void setRows(List<List<Object>> rows) { this.rows = rows; }

    public int getRowCount() { return rowCount; }
    public void setRowCount(int rowCount) { this.rowCount = rowCount; }

    public long getExecutionTimeMs() { return executionTimeMs; }
    public void setExecutionTimeMs(long executionTimeMs) { this.executionTimeMs = executionTimeMs; }

    public boolean isTruncated() { return truncated; }
    public void setTruncated(boolean truncated) { this.truncated = truncated; }

    public static class ColumnInfo {
        private String name;
        private String dataType;

        public String getName() { return name; }
        public void setName(String name) { this.name = name; }

        public String getDataType() { return dataType; }
        public void setDataType(String dataType) { this.dataType = dataType; }
    }
}

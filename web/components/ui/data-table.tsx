"use client"

import * as React from "react"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "./table"
import { Button } from "./button"
import { Input } from "./input"
import { Checkbox } from "./checkbox"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
  DropdownMenuCheckboxItem,
} from "./dropdown-menu"
import { Badge } from "./badge"
import { cn } from "@/lib/utils"
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Search,
  Filter,
  ArrowUpDown,
  ArrowUp,
  ArrowDown,
  MoreHorizontal,
  Download,
  Trash2,
  Settings2,
} from "lucide-react"

export interface Column<T> {
  id: string
  header: string | React.ReactNode
  accessorKey?: keyof T
  accessorFn?: (row: T) => React.ReactNode
  cell?: (row: T) => React.ReactNode
  sortable?: boolean
  filterable?: boolean
  className?: string
  headerClassName?: string
}

interface DataTableProps<T> {
  data: T[]
  columns: Column<T>[]
  isLoading?: boolean
  loadingRows?: number
  emptyState?: React.ReactNode
  searchPlaceholder?: string
  searchKey?: keyof T
  onSearch?: (query: string) => void
  pageSize?: number
  pageSizeOptions?: number[]
  selectable?: boolean
  onSelectionChange?: (selectedRows: T[]) => void
  bulkActions?: {
    label: string
    icon?: React.ReactNode
    onClick: (selectedRows: T[]) => void
    variant?: "default" | "destructive"
  }[]
  rowActions?: (row: T) => {
    label: string
    icon?: React.ReactNode
    onClick: () => void
    variant?: "default" | "destructive"
  }[]
  onRowClick?: (row: T) => void
  getRowId?: (row: T) => string
  className?: string
  stickyHeader?: boolean
  compact?: boolean
}

export function DataTable<T extends Record<string, any>>({
  data,
  columns,
  isLoading = false,
  loadingRows = 5,
  emptyState,
  searchPlaceholder = "Search...",
  searchKey,
  onSearch,
  pageSize: initialPageSize = 10,
  pageSizeOptions = [10, 25, 50, 100],
  selectable = false,
  onSelectionChange,
  bulkActions,
  rowActions,
  onRowClick,
  getRowId = (row) => JSON.stringify(row),
  className,
  stickyHeader = false,
  compact = false
}: DataTableProps<T>) {
  const [searchQuery, setSearchQuery] = React.useState("")
  const [currentPage, setCurrentPage] = React.useState(1)
  const [pageSize, setPageSize] = React.useState(initialPageSize)
  const [sortConfig, setSortConfig] = React.useState<{
    key: string
    direction: "asc" | "desc"
  } | null>(null)
  const [selectedRows, setSelectedRows] = React.useState<Set<string>>(new Set())
  const [visibleColumns, setVisibleColumns] = React.useState<Set<string>>(
    new Set(columns.map((c) => c.id))
  )

  // Filter data based on search
  const filteredData = React.useMemo(() => {
    if (!searchQuery || !searchKey) return data
    return data.filter((row) => {
      const value = row[searchKey]
      if (typeof value === "string") {
        return value.toLowerCase().includes(searchQuery.toLowerCase())
      }
      return String(value).toLowerCase().includes(searchQuery.toLowerCase())
    })
  }, [data, searchQuery, searchKey])

  // Sort data
  const sortedData = React.useMemo(() => {
    if (!sortConfig) return filteredData
    return [...filteredData].sort((a, b) => {
      const col = columns.find((c) => c.id === sortConfig.key)
      if (!col) return 0

      const aValue = col.accessorKey ? a[col.accessorKey] : null
      const bValue = col.accessorKey ? b[col.accessorKey] : null

      if (aValue === null || aValue === undefined) return 1
      if (bValue === null || bValue === undefined) return -1

      if (typeof aValue === "number" && typeof bValue === "number") {
        return sortConfig.direction === "asc" ? aValue - bValue : bValue - aValue
      }

      const aStr = String(aValue).toLowerCase()
      const bStr = String(bValue).toLowerCase()
      return sortConfig.direction === "asc"
        ? aStr.localeCompare(bStr)
        : bStr.localeCompare(aStr)
    })
  }, [filteredData, sortConfig, columns])

  // Paginate data
  const paginatedData = React.useMemo(() => {
    const start = (currentPage - 1) * pageSize
    return sortedData.slice(start, start + pageSize)
  }, [sortedData, currentPage, pageSize])

  const totalPages = Math.ceil(sortedData.length / pageSize)

  // Reset page when search changes
  React.useEffect(() => {
    setCurrentPage(1)
  }, [searchQuery])

  // Notify parent of selection changes
  React.useEffect(() => {
    if (onSelectionChange) {
      const selected = data.filter((row) => selectedRows.has(getRowId(row)))
      onSelectionChange(selected)
    }
  }, [selectedRows, data, getRowId, onSelectionChange])

  const toggleSort = (columnId: string) => {
    setSortConfig((prev) => {
      if (prev?.key !== columnId) return { key: columnId, direction: "asc" }
      if (prev.direction === "asc") return { key: columnId, direction: "desc" }
      return null
    })
  }

  const toggleRowSelection = (rowId: string) => {
    setSelectedRows((prev) => {
      const next = new Set(prev)
      if (next.has(rowId)) {
        next.delete(rowId)
      } else {
        next.add(rowId)
      }
      return next
    })
  }

  const toggleAllSelection = () => {
    if (selectedRows.size === paginatedData.length) {
      setSelectedRows(new Set())
    } else {
      setSelectedRows(new Set(paginatedData.map(getRowId)))
    }
  }

  const handleSearch = (value: string) => {
    setSearchQuery(value)
    onSearch?.(value)
  }

  const visibleCols = columns.filter((c) => visibleColumns.has(c.id))

  return (
    <div className={cn("space-y-4", className)}>
      {/* Toolbar */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-1 items-center gap-2">
          {searchKey && (
            <div className="relative max-w-sm">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                placeholder={searchPlaceholder}
                value={searchQuery}
                onChange={(e) => handleSearch(e.target.value)}
                className="pl-9"
              />
            </div>
          )}

          {/* Bulk actions */}
          {selectable && selectedRows.size > 0 && bulkActions && (
            <div className="flex items-center gap-2">
              <Badge variant="secondary">{selectedRows.size} selected</Badge>
              {bulkActions.map((action, i) => (
                <Button
                  key={i}
                  variant={action.variant === "destructive" ? "destructive" : "outline"}
                  size="sm"
                  onClick={() => {
                    const selected = data.filter((row) => selectedRows.has(getRowId(row)))
                    action.onClick(selected)
                  }}
                >
                  {action.icon}
                  {action.label}
                </Button>
              ))}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2">
          {/* Column visibility */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm">
                <Settings2 className="mr-2 h-4 w-4" />
                Columns
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              {columns.map((column) => (
                <DropdownMenuCheckboxItem
                  key={column.id}
                  checked={visibleColumns.has(column.id)}
                  onCheckedChange={(checked) => {
                    setVisibleColumns((prev) => {
                      const next = new Set(prev)
                      if (checked) {
                        next.add(column.id)
                      } else {
                        next.delete(column.id)
                      }
                      return next
                    })
                  }}
                >
                  {typeof column.header === "string" ? column.header : column.id}
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {/* Table */}
      <div className={cn("rounded-md border", stickyHeader && "max-h-[600px] overflow-auto")}>
        <Table>
          <TableHeader className={cn(stickyHeader && "sticky top-0 bg-background z-10")}>
            <TableRow>
              {selectable && (
                <TableHead className="w-12">
                  <Checkbox
                    checked={paginatedData.length > 0 && selectedRows.size === paginatedData.length}
                    onCheckedChange={toggleAllSelection}
                  />
                </TableHead>
              )}
              {visibleCols.map((column) => (
                <TableHead
                  key={column.id}
                  className={cn(column.headerClassName, compact && "py-2")}
                >
                  {column.sortable ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="-ml-3 h-8"
                      onClick={() => toggleSort(column.id)}
                    >
                      {column.header}
                      {sortConfig?.key === column.id ? (
                        sortConfig.direction === "asc" ? (
                          <ArrowUp className="ml-2 h-4 w-4" />
                        ) : (
                          <ArrowDown className="ml-2 h-4 w-4" />
                        )
                      ) : (
                        <ArrowUpDown className="ml-2 h-4 w-4 opacity-50" />
                      )}
                    </Button>
                  ) : (
                    column.header
                  )}
                </TableHead>
              ))}
              {rowActions && <TableHead className="w-12" />}
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              Array.from({ length: loadingRows }).map((_, i) => (
                <TableRow key={i}>
                  {selectable && (
                    <TableCell>
                      <div className="h-4 w-4 animate-pulse rounded bg-muted" />
                    </TableCell>
                  )}
                  {visibleCols.map((col) => (
                    <TableCell key={col.id}>
                      <div className="h-4 w-24 animate-pulse rounded bg-muted" />
                    </TableCell>
                  ))}
                  {rowActions && (
                    <TableCell>
                      <div className="h-4 w-8 animate-pulse rounded bg-muted" />
                    </TableCell>
                  )}
                </TableRow>
              ))
            ) : paginatedData.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={visibleCols.length + (selectable ? 1 : 0) + (rowActions ? 1 : 0)}
                  className="h-48"
                >
                  {emptyState || (
                    <div className="text-center text-muted-foreground">
                      No data available
                    </div>
                  )}
                </TableCell>
              </TableRow>
            ) : (
              paginatedData.map((row) => {
                const rowId = getRowId(row)
                return (
                  <TableRow
                    key={rowId}
                    data-state={selectedRows.has(rowId) ? "selected" : undefined}
                    className={cn(onRowClick && "cursor-pointer")}
                    onClick={() => onRowClick?.(row)}
                  >
                    {selectable && (
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <Checkbox
                          checked={selectedRows.has(rowId)}
                          onCheckedChange={() => toggleRowSelection(rowId)}
                        />
                      </TableCell>
                    )}
                    {visibleCols.map((column) => (
                      <TableCell key={column.id} className={cn(column.className, compact && "py-2")}>
                        {column.cell
                          ? column.cell(row)
                          : column.accessorFn
                          ? column.accessorFn(row)
                          : column.accessorKey
                          ? String(row[column.accessorKey] ?? "")
                          : null}
                      </TableCell>
                    ))}
                    {rowActions && (
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="icon" className="h-8 w-8">
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {rowActions(row).map((action, i) => (
                              <DropdownMenuItem
                                key={i}
                                onClick={action.onClick}
                                className={cn(
                                  action.variant === "destructive" && "text-destructive"
                                )}
                              >
                                {action.icon}
                                {action.label}
                              </DropdownMenuItem>
                            ))}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </TableCell>
                    )}
                  </TableRow>
                )
              })
            )}
          </TableBody>
        </Table>
      </div>

      {/* Pagination */}
      {!isLoading && sortedData.length > 0 && (
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="text-sm text-muted-foreground">
            Showing {(currentPage - 1) * pageSize + 1} to{" "}
            {Math.min(currentPage * pageSize, sortedData.length)} of {sortedData.length} results
          </div>
          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1 text-sm">
              <span className="text-muted-foreground">Rows per page:</span>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="sm" className="w-16">
                    {pageSize}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {pageSizeOptions.map((size) => (
                    <DropdownMenuItem
                      key={size}
                      onClick={() => {
                        setPageSize(size)
                        setCurrentPage(1)
                      }}
                    >
                      {size}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
            <div className="flex items-center gap-1">
              <Button
                variant="outline"
                size="icon"
                className="h-8 w-8"
                onClick={() => setCurrentPage(1)}
                disabled={currentPage === 1}
              >
                <ChevronsLeft className="h-4 w-4" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                className="h-8 w-8"
                onClick={() => setCurrentPage((p) => Math.max(1, p - 1))}
                disabled={currentPage === 1}
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
              <span className="px-2 text-sm">
                Page {currentPage} of {totalPages || 1}
              </span>
              <Button
                variant="outline"
                size="icon"
                className="h-8 w-8"
                onClick={() => setCurrentPage((p) => Math.min(totalPages, p + 1))}
                disabled={currentPage === totalPages}
              >
                <ChevronRight className="h-4 w-4" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                className="h-8 w-8"
                onClick={() => setCurrentPage(totalPages)}
                disabled={currentPage === totalPages}
              >
                <ChevronsRight className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

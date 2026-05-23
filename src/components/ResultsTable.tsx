import { useState, useMemo, useEffect } from "react";
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  flexRender,
  createColumnHelper,
  SortingState,
} from "@tanstack/react-table";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ArrowUpDown,
  ArrowUp,
  ArrowDown,
  Copy,
  Check,
  Clock,
} from "lucide-react";
import type { QueryResult } from "../types";
import { ResultActions } from "./ResultActions";

interface ResultsTableProps {
  result: QueryResult | null;
  onApplySql?: (sql: string) => void;
}

interface ContextMenuState {
  x: number;
  y: number;
  value: string;
  column: string;
}

export function ResultsTable({ result, onApplySql }: ResultsTableProps) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [pageIndex, setPageIndex] = useState(0);
  const pageSize = 50;
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [copied, setCopied] = useState(false);

  // Close context menu on click outside
  useEffect(() => {
    const handleClick = () => setContextMenu(null);
    if (contextMenu) {
      window.addEventListener("click", handleClick);
      return () => window.removeEventListener("click", handleClick);
    }
  }, [contextMenu]);

  const handleCopy = async (value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
    setContextMenu(null);
  };

  const columns = useMemo(() => {
    if (!result || result.columns.length === 0) return [];
    const helper = createColumnHelper<any>();
    return result.columns.map((col) =>
      helper.accessor(
        (row: Record<string, unknown>) => row[col],
        {
          id: col,
          header: col,
          cell: (info) => {
            const raw = info.getValue();
            if (raw === null || raw === undefined) {
              return <span className="text-[var(--text-muted)] italic">NULL</span>;
            }
            return <span>{String(raw)}</span>;
          },
        }
      )
    );
  }, [result]);

  const data = useMemo(() => {
    if (!result) return [];
    return result.rows.map((row) => {
      const obj: Record<string, unknown> = {};
      result.columns.forEach((col, i) => {
        obj[col] = row[i];
      });
      return obj;
    });
  }, [result]);

  const table = useReactTable({
    data,
    columns,
    state: {
      sorting,
      pagination: {
        pageIndex,
        pageSize,
      },
    },
    onSortingChange: setSorting,
    onPaginationChange: (updater) => {
      if (typeof updater === "function") {
        const newState = updater({ pageIndex, pageSize });
        setPageIndex(newState.pageIndex);
      }
    },
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
  });

  if (!result || result.columns.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--text-muted)]">
        No results to display
      </div>
    );
  }

  const totalPages = table.getPageCount();
  const currentPage = table.getState().pagination.pageIndex + 1;

  // Format execution time
  const execTime = result.execution_time_ms;
  const timeStr = execTime < 1000 ? `${execTime}ms` : `${(execTime / 1000).toFixed(1)}s`;

  // Status badge
  const isWrite = result.affected_rows !== null && result.affected_rows !== undefined;

  return (
    <div className="space-y-3">
      {/* Results header with execution stats */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-sm text-[var(--text-secondary)]">
            {isWrite
              ? `${result.affected_rows} row${result.affected_rows !== 1 ? "s" : ""} affected`
              : `${result.row_count} row${result.row_count !== 1 ? "s" : ""} returned`}
          </span>
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-[var(--bg-secondary)] border border-[var(--border)] text-xs text-[var(--text-muted)]">
            <Clock className="w-3 h-3" />
            {timeStr}
          </span>
        </div>
        {totalPages > 1 && (
          <span className="text-sm text-[var(--text-muted)]">
            Page {currentPage} of {totalPages}
          </span>
        )}
      </div>

      {/* Table */}
      <div className="overflow-auto rounded-lg border border-[var(--border)]">
        <table className="w-full text-sm">
          <thead className="bg-[var(--bg-secondary)] sticky top-0">
            {table.getHeaderGroups().map((headerGroup) => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <th
                    key={header.id}
                    onClick={header.column.getToggleSortingHandler()}
                    className="px-3 py-2 text-left font-medium text-[var(--text-secondary)] border-b border-[var(--border)] cursor-pointer hover:bg-[var(--bg-tertiary)] transition-colors whitespace-nowrap select-none"
                  >
                    <div className="flex items-center gap-1">
                      {flexRender(header.column.columnDef.header, header.getContext())}
                      <span className="text-[var(--text-muted)]">
                        {header.column.getIsSorted() === "asc" ? (
                          <ArrowUp className="w-3 h-3" />
                        ) : header.column.getIsSorted() === "desc" ? (
                          <ArrowDown className="w-3 h-3" />
                        ) : (
                          <ArrowUpDown className="w-3 h-3 opacity-0 group-hover:opacity-100" />
                        )}
                      </span>
                    </div>
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row) => (
              <tr key={row.id} className="hover:bg-[var(--bg-secondary)] transition-colors">
                {row.getVisibleCells().map((cell) => {
                  const raw = row.original[cell.column.id];
                  const displayValue = raw === null || raw === undefined ? "" : String(raw);
                  return (
                    <td
                      key={cell.id}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setContextMenu({
                          x: e.clientX,
                          y: e.clientY,
                          value: raw === null || raw === undefined ? "NULL" : String(raw),
                          column: cell.column.id,
                        });
                      }}
                      className="px-3 py-2 border-b border-[var(--border)] text-[var(--text-primary)] whitespace-nowrap max-w-[300px] truncate"
                      title={displayValue}
                    >
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="fixed z-50 min-w-[160px] rounded-lg bg-[var(--bg-primary)] border border-[var(--border)] shadow-xl py-1"
          style={{ top: contextMenu.y, left: contextMenu.x }}
        >
          <button
            onClick={() => handleCopy(contextMenu.value)}
            className="w-full px-3 py-2 text-sm text-left hover:bg-[var(--bg-secondary)] flex items-center gap-2 text-[var(--text-primary)]"
          >
            {copied ? (
              <Check className="w-3.5 h-3.5 text-[var(--success)]" />
            ) : (
              <Copy className="w-3.5 h-3.5 text-[var(--text-muted)]" />
            )}
            {copied ? "Copied!" : "Copy Value"}
          </button>
          <button
            onClick={() => handleCopy(contextMenu.column)}
            className="w-full px-3 py-2 text-sm text-left hover:bg-[var(--bg-secondary)] flex items-center gap-2 text-[var(--text-primary)]"
          >
            <Copy className="w-3.5 h-3.5 text-[var(--text-muted)]" />
            Copy Column Name
          </button>
          <div className="my-1 border-t border-[var(--border)]" />
          <div className="px-3 py-1.5 text-xs text-[var(--text-muted)] max-w-[200px] truncate font-mono">
            {contextMenu.value}
          </div>
        </div>
      )}

      {/* AI Result Actions */}
      <ResultActions
        columns={result.columns}
        rows={result.rows}
        rowCount={result.row_count}
        onApplySql={onApplySql || (() => {})}
      />

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <button
              onClick={() => setPageIndex(0)}
              disabled={!table.getCanPreviousPage()}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronsLeft className="w-4 h-4 text-[var(--text-secondary)]" />
            </button>
            <button
              onClick={() => setPageIndex((p) => Math.max(0, p - 1))}
              disabled={!table.getCanPreviousPage()}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronLeft className="w-4 h-4 text-[var(--text-secondary)]" />
            </button>
            <button
              onClick={() => setPageIndex((p) => Math.min(totalPages - 1, p + 1))}
              disabled={!table.getCanNextPage()}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronRight className="w-4 h-4 text-[var(--text-secondary)]" />
            </button>
            <button
              onClick={() => setPageIndex(totalPages - 1)}
              disabled={!table.getCanNextPage()}
              className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronsRight className="w-4 h-4 text-[var(--text-secondary)]" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

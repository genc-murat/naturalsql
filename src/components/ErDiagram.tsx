import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import {
  X,
  ZoomIn,
  ZoomOut,
  Maximize2,
  Loader2,
  Key,
  Link2,
  AlertCircle,
  Table2,
  Move,
  MousePointer2,
  Minimize2,
  Map,
  ImageDown,
  Grid3X3,
  Layers,
  Palette,
} from "lucide-react";
import { getErDiagramData } from "../api";
import type { ErTableNode, ErRelation } from "../types";
import { toPng } from "html-to-image";

interface ErDiagramProps {
  isOpen: boolean;
  onClose: () => void;
  database: string;
  onViewData?: (database: string, table: string) => void;
}

interface TablePosition {
  x: number;
  y: number;
}

// These are used in getTableSize but keep for clarity
const TABLE_WIDTH = 260;
const HEADER_HEIGHT = 42;
const COLUMN_HEIGHT = 32;
const GRID_SPACING_X = 380;
const GRID_SPACING_Y = 320;
const PADDING = 100;

// Database color palette for schema-based table coloring
const DATABASE_COLORS = [
  { hue: 240, sat: 60, label: "Indigo" },
  { hue: 38, sat: 92, label: "Amber" },
  { hue: 160, sat: 84, label: "Emerald" },
  { hue: 330, sat: 67, label: "Pink" },
  { hue: 200, sat: 90, label: "Sky" },
  { hue: 270, sat: 80, label: "Violet" },
  { hue: 24, sat: 90, label: "Orange" },
  { hue: 180, sat: 70, label: "Teal" },
  { hue: 0, sat: 70, label: "Red" },
  { hue: 80, sat: 70, label: "Lime" },
];

function getDatabaseColor(database: string): { hue: number; sat: number; label: string } {
  // Hash the database name to pick a consistent color
  let hash = 0;
  for (let i = 0; i < database.length; i++) {
    hash = ((hash << 5) - hash) + database.charCodeAt(i);
    hash |= 0;
  }
  const idx = Math.abs(hash) % DATABASE_COLORS.length;
  return DATABASE_COLORS[idx];
}

export function ErDiagram({ isOpen, onClose, database, onViewData }: ErDiagramProps) {
  const [tables, setTables] = useState<ErTableNode[]>([]);
  const [relations, setRelations] = useState<ErRelation[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [positions, setPositions] = useState<Record<string, TablePosition>>({});
  const [dragging, setDragging] = useState<{ table: string; startX: number; startY: number; tableX: number; tableY: number } | null>(null);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isPanning, setIsPanning] = useState(false);
  const [panStart, setPanStart] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(0.85);
  const [hoveredRelation, setHoveredRelation] = useState<string | null>(null);
  const [hoveredTable, setHoveredTable] = useState<string | null>(null);
  const [selectedTable, setSelectedTable] = useState<string | null>(null);
  const [showMinimap, setShowMinimap] = useState(true);
  const [collapsedTables, setCollapsedTables] = useState<Record<string, boolean>>({});
  const [exporting, setExporting] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const minimapRef = useRef<HTMLDivElement>(null);
  const isDraggingMinimap = useRef(false);

  // Load data
  useEffect(() => {
    if (!isOpen || !database) return;
    setLoading(true);
    setError("");
    getErDiagramData(database)
      .then((res) => {
        setTables(res.tables);
        setRelations(res.relations);

        // Try to restore saved positions from localStorage
        const storageKey = `er-diagram-positions-${database}`;
        const savedRaw = localStorage.getItem(storageKey);
        let savedPositions: Record<string, TablePosition> | null = null;
        if (savedRaw) {
          try {
            const parsed = JSON.parse(savedRaw);
            // Validate: only use saved positions for tables that still exist
            const valid: Record<string, TablePosition> = {};
            let hasAny = false;
            for (const t of res.tables) {
              if (parsed[t.table]) {
                valid[t.table] = parsed[t.table];
                hasAny = true;
              }
            }
            if (hasAny) savedPositions = valid;
          } catch {}
        }

        // Calculate initial grid positions for tables without saved positions
        const pos: Record<string, TablePosition> = {};
        const cols = Math.ceil(Math.sqrt(res.tables.length));
        res.tables.forEach((t: ErTableNode, i: number) => {
          if (savedPositions?.[t.table]) {
            pos[t.table] = savedPositions[t.table];
          } else {
            const col = i % cols;
            const row = Math.floor(i / cols);
            pos[t.table] = {
              x: PADDING + col * GRID_SPACING_X,
              y: PADDING + row * GRID_SPACING_Y,
            };
          }
        });
        setPositions(pos);
        setSelectedTable(null);
        setHoveredTable(null);
        setHoveredRelation(null);
        // Center on content
        const maxRow = Math.ceil(res.tables.length / cols);
        const totalW = cols * GRID_SPACING_X + PADDING * 2;
        const totalH = maxRow * GRID_SPACING_Y + PADDING * 2;
        const container = containerRef.current;
        if (container) {
          const cx = (totalW - container.clientWidth / zoom) / 2;
          const cy = (totalH - container.clientHeight / zoom) / 2;
          setPan({ x: -cx, y: -cy });
        }
      })
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setLoading(false));
  }, [isOpen, database]);

  // Compute table height based on columns
  const getTableHeight = useCallback((table: ErTableNode, isCollapsed?: boolean) => {
    if (isCollapsed) return HEADER_HEIGHT + 2;
    return HEADER_HEIGHT + table.columns.length * COLUMN_HEIGHT + 8;
  }, []);

  // Auto-fit when tables load
  const fitToScreen = useCallback(() => {
    const container = containerRef.current;
    if (!container || tables.length === 0) return;

    const minX = Math.min(...Object.values(positions).map((p) => p.x));
    const minY = Math.min(...Object.values(positions).map((p) => p.y));
    const maxX = Math.max(...Object.values(positions).map((p) => p.x + TABLE_WIDTH));
    const maxY = Math.max(...Object.values(positions).map((p) => p.y + 400));

    const contentW = maxX - minX + PADDING;
    const contentH = maxY - minY + PADDING;
    const containerW = container.clientWidth;
    const containerH = container.clientHeight;

    const fitZoom = Math.min(containerW / contentW, containerH / contentH, 1.2);
    const newZoom = Math.max(0.3, Math.min(fitZoom, 2));

    setZoom(newZoom);
    setPan({
      x: (containerW - contentW * newZoom) / 2 - minX * newZoom + PADDING * newZoom / 2,
      y: (containerH - contentH * newZoom) / 2 - minY * newZoom + PADDING * newZoom / 2,
    });
  }, [tables, positions]);

  // Save positions to localStorage (debounced)
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!database || tables.length === 0) return;
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      try {
        localStorage.setItem(`er-diagram-positions-${database}`, JSON.stringify(positions));
      } catch {}
    }, 500);
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, [positions, database, tables.length]);

  // Reset layout to default grid
  const handleResetLayout = useCallback(() => {
    const pos: Record<string, TablePosition> = {};
    const cols = Math.ceil(Math.sqrt(tables.length));
    tables.forEach((t, i) => {
      const col = i % cols;
      const row = Math.floor(i / cols);
      pos[t.table] = {
        x: PADDING + col * GRID_SPACING_X,
        y: PADDING + row * GRID_SPACING_Y,
      };
    });
    setPositions(pos);
    // Clear saved positions
    try {
      localStorage.removeItem(`er-diagram-positions-${database}`);
    } catch {}
    fitToScreen();
  }, [tables, database, fitToScreen]);

  // Auto layout based on FK relationships
  const handleAutoLayout = useCallback(() => {
    if (tables.length === 0) return;

    // Build adjacency graph from FK relations using plain objects
    const graph: Record<string, Set<string>> = {};
    for (const t of tables) {
      graph[t.table] = new Set();
    }
    for (const rel of relations) {
      graph[rel.from_table]?.add(rel.to_table);
      graph[rel.to_table]?.add(rel.from_table);
    }

    // Find the most connected table as root
    let root = tables[0].table;
    let maxDegree = 0;
    for (const [table, neighbors] of Object.entries(graph)) {
      if (neighbors.size > maxDegree) {
        maxDegree = neighbors.size;
        root = table;
      }
    }

    // BFS to assign layers (distance from root)
    const layers: Record<string, number> = {};
    const queue: string[] = [root];
    layers[root] = 0;
    while (queue.length > 0) {
      const current = queue.shift()!;
      const currentLayer = layers[current]!;
      for (const neighbor of graph[current] || []) {
        if (layers[neighbor] === undefined) {
          layers[neighbor] = currentLayer + 1;
          queue.push(neighbor);
        }
      }
    }

    // Assign unconnected tables to layer 0
    for (const t of tables) {
      if (layers[t.table] === undefined) {
        layers[t.table] = 0;
      }
    }

    // Group tables by layer
    const layerGroups: Record<number, ErTableNode[]> = {};
    for (const t of tables) {
      const layer = layers[t.table] ?? 0;
      if (!layerGroups[layer]) {
        layerGroups[layer] = [];
      }
      layerGroups[layer].push(t);
    }

    // Sort layers and sort tables within each layer by connection count
    const sortedLayers = Object.entries(layerGroups).sort(
      (a: [string, ErTableNode[]], b: [string, ErTableNode[]]) => Number(a[0]) - Number(b[0])
    );

    const LAYER_GAP_X = GRID_SPACING_X + 80;
    const ITEM_GAP_Y = GRID_SPACING_Y;

    const pos: Record<string, TablePosition> = {};
    for (let layerIdx = 0; layerIdx < sortedLayers.length; layerIdx++) {
      const layerTables = sortedLayers[layerIdx][1];
      // Sort by degree (most connected first)
      layerTables.sort((a: ErTableNode, b: ErTableNode) => (graph[b.table]?.size ?? 0) - (graph[a.table]?.size ?? 0));
      const startY = PADDING;
      layerTables.forEach((t: ErTableNode, i: number) => {
        pos[t.table] = {
          x: PADDING + layerIdx * LAYER_GAP_X,
          y: startY + i * ITEM_GAP_Y,
        };
      });
    }

    setPositions(pos);
    try {
      localStorage.removeItem(`er-diagram-positions-${database}`);
    } catch {}

    // Calculate and apply fit-to-screen inline using the new positions
    const container = containerRef.current;
    if (container) {
      const posValues = Object.values(pos);
      const px = Math.min(...posValues.map((p: TablePosition) => p.x));
      const py = Math.min(...posValues.map((p: TablePosition) => p.y));
      const pMaxX = Math.max(...posValues.map((p: TablePosition) => p.x + TABLE_WIDTH));
      const pMaxY = Math.max(...posValues.map((p: TablePosition) => p.y + 400));
      const cw = pMaxX - px + PADDING;
      const ch = pMaxY - py + PADDING;
      const fz = Math.min(container.clientWidth / cw, container.clientHeight / ch, 1.2);
      const nz = Math.max(0.3, Math.min(fz, 2));
      setZoom(nz);
      setPan({
        x: (container.clientWidth - cw * nz) / 2 - px * nz + PADDING * nz / 2,
        y: (container.clientHeight - ch * nz) / 2 - py * nz + PADDING * nz / 2,
      });
    }
  }, [tables, relations, database]);

  // Export as PNG
  const handleExportPng = useCallback(async () => {
    const container = containerRef.current;
    if (!container || tables.length === 0) return;

    setExporting(true);

    try {
      // Save current pan/zoom state
      const savedPan = { ...pan };
      const savedZoom = zoom;

      // Fit the entire content into view temporarily
      const minX = Math.min(...Object.values(positions).map((p) => p.x));
      const minY = Math.min(...Object.values(positions).map((p) => p.y));
      const maxX = Math.max(...Object.values(positions).map((p) => p.x + TABLE_WIDTH));
      const contentW = maxX - minX + PADDING;
      // Target export size (max 4096px wide)
      const targetW = Math.min(contentW, 4096);
      const exportZoom = targetW / contentW;
      setZoom(exportZoom);
      setPan({
        x: -minX * exportZoom + PADDING * exportZoom / 2,
        y: -minY * exportZoom + PADDING * exportZoom / 2,
      });

      // Wait for re-render
      await new Promise((resolve) => requestAnimationFrame(resolve));
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Capture
      const dataUrl = await toPng(container, {
        backgroundColor: "#1a1b2e",
        pixelRatio: 2,
        quality: 1,
        cacheBust: true,
      });

      // Download
      const link = document.createElement("a");
      link.download = `er-diagram-${database}-${new Date().toISOString().slice(0, 10)}.png`;
      link.href = dataUrl;
      link.click();

      // Restore
      setPan(savedPan);
      setZoom(savedZoom);
    } catch (err) {
      console.error("Failed to export PNG:", err);
    } finally {
      setExporting(false);
    }
  }, [containerRef, tables, positions, pan, zoom, database]);

  // Transform a content-space coordinate to screen-space
  // SVG line between two tables for a relation (returns content-space coordinates)
  const getRelationPath = useCallback((rel: ErRelation) => {
    const fromPos = positions[rel.from_table];
    const toPos = positions[rel.to_table];
    if (!fromPos || !toPos) return "";

    const toTable = tables.find((t) => t.table === rel.to_table);
    const fromTable = tables.find((t) => t.table === rel.from_table);
    if (!toTable || !fromTable) return "";

    const isToCollapsed = !!collapsedTables[rel.to_table];
    const isFromCollapsed = !!collapsedTables[rel.from_table];
    const toH = getTableHeight(toTable, isToCollapsed);

    // Find which column index this FK is on
    const colIdx = fromTable.columns.findIndex((c) => c.name === rel.from_column);
    const effectiveColIdx = colIdx >= 0 ? colIdx : 0;
    // When source table is collapsed, center the connection in the header
    const colYOffset = isFromCollapsed
      ? HEADER_HEIGHT / 2
      : HEADER_HEIGHT + effectiveColIdx * COLUMN_HEIGHT + COLUMN_HEIGHT / 2;

    // Content-space coordinates
    const fromX = fromPos.x + TABLE_WIDTH;
    const fromY = fromPos.y + colYOffset;
    const toX = toPos.x;
    const toY = toPos.y + toH / 2;

    const dx = Math.abs(toX - fromX);
    const cpOffset = Math.max(dx * 0.5, 60);

    return `M ${fromX} ${fromY} C ${fromX + cpOffset} ${fromY}, ${toX - cpOffset} ${toY}, ${toX} ${toY}`;
  }, [positions, tables, getTableHeight, collapsedTables]);

  // Get arrowhead position at end of curve (content-space)
  const getArrowPosition = useCallback((rel: ErRelation) => {
    const fromPos = positions[rel.from_table];
    const toPos = positions[rel.to_table];
    if (!fromPos || !toPos) return { x: 0, y: 0, angle: 0 };

    const toTable = tables.find((t) => t.table === rel.to_table);
    const fromTable = tables.find((t) => t.table === rel.from_table);
    if (!toTable || !fromTable) return { x: 0, y: 0, angle: 0 };

    const isToCollapsed = !!collapsedTables[rel.to_table];
    const isFromCollapsed = !!collapsedTables[rel.from_table];
    const toH = getTableHeight(toTable, isToCollapsed);

    const colIdx = fromTable.columns.findIndex((c) => c.name === rel.from_column);
    const effectiveColIdx = colIdx >= 0 ? colIdx : 0;
    // When source table is collapsed, center the connection in the header
    const colYOffset = isFromCollapsed
      ? HEADER_HEIGHT / 2
      : HEADER_HEIGHT + effectiveColIdx * COLUMN_HEIGHT + COLUMN_HEIGHT / 2;

    // Content-space coordinates
    const fromX = fromPos.x + TABLE_WIDTH;
    const fromY = fromPos.y + colYOffset;
    const toX = toPos.x;
    const toY = toPos.y + toH / 2;

    const angle = Math.atan2(toY - fromY, toX - fromX);
    return { x: toX, y: toY, angle };
  }, [positions, tables, getTableHeight, collapsedTables]);

  // Mouse handlers for table dragging
  const handleTableMouseDown = (e: React.MouseEvent, table: string) => {
    e.stopPropagation();
    if (e.button !== 0) return;
    setDragging({
      table,
      startX: e.clientX,
      startY: e.clientY,
      tableX: positions[table].x,
      tableY: positions[table].y,
    });
  };

  useEffect(() => {
    if (!dragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const dx = (e.clientX - dragging.startX) / zoom;
      const dy = (e.clientY - dragging.startY) / zoom;
      setPositions((prev) => ({
        ...prev,
        [dragging.table]: {
          x: dragging.tableX + dx,
          y: dragging.tableY + dy,
        },
      }));
    };

    const handleMouseUp = () => setDragging(null);

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [dragging, zoom]);

  // Pan handlers
  const handleBgMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0 || dragging) return;
    if ((e.target as HTMLElement).closest("[data-table-box]")) return;
    setIsPanning(true);
    setPanStart({ x: e.clientX - pan.x, y: e.clientY - pan.y });
  };

  useEffect(() => {
    if (!isPanning) return;

    const handleMouseMove = (e: MouseEvent) => {
      setPan({ x: e.clientX - panStart.x, y: e.clientY - panStart.y });
    };

    const handleMouseUp = () => setIsPanning(false);

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isPanning, panStart]);

  // Minimap drag handler - use refs to avoid re-registering listeners
  const panRef = useRef(pan);
  const zoomRef = useRef(zoom);
  panRef.current = pan;
  zoomRef.current = zoom;

  useEffect(() => {
    if (!minimapRef.current?.querySelector(".minimap-canvas")) return;

    const handleMinimapMove = (e: MouseEvent) => {
      if (!isDraggingMinimap.current) return;
      e.preventDefault();
      const el = minimapRef.current?.querySelector(".minimap-canvas");
      if (!el) return;
      const rect = el.getBoundingClientRect();

      // Calculate content bounds for scale
      const tableBounds = tables.map(t => {
        const p = positions[t.table];
        const h = getTableHeight(t);
        return p ? { x: p.x, y: p.y, w: TABLE_WIDTH, h } : null;
      }).filter(Boolean) as { x: number; y: number; w: number; h: number }[];
      if (tableBounds.length === 0) return;

      const minX = Math.min(...tableBounds.map(b => b.x));
      const minY = Math.min(...tableBounds.map(b => b.y));
      const maxX = Math.max(...tableBounds.map(b => b.x + b.w));
      const maxY = Math.max(...tableBounds.map(b => b.y + b.h));
      const contentW = maxX - minX + 40;
      const contentH = maxY - minY + 40;
      const MINIMAP_W = 180;
      const MINIMAP_H = 120;
      const scale = Math.min(MINIMAP_W / contentW, MINIMAP_H / contentH, 0.3);
      const offsetX = (MINIMAP_W - contentW * scale) / 2 - minX * scale + 20 * scale;
      const offsetY = (MINIMAP_H - contentH * scale) / 2 - minY * scale + 20 * scale;

      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const targetX = (mx - offsetX) / scale;
      const targetY = (my - offsetY) / scale;

      const containerEl = containerRef.current;
      if (containerEl) {
        const newPanX = -(targetX - containerEl.clientWidth / (2 * zoomRef.current));
        const newPanY = -(targetY - containerEl.clientHeight / (2 * zoomRef.current));
        setPan({ x: newPanX, y: newPanY });
      }
    };

    const handleMinimapUp = () => {
      isDraggingMinimap.current = false;
    };

    window.addEventListener("mousemove", handleMinimapMove);
    window.addEventListener("mouseup", handleMinimapUp);
    return () => {
      window.removeEventListener("mousemove", handleMinimapMove);
      window.removeEventListener("mouseup", handleMinimapUp);
    };
  }, [tables, positions, getTableHeight]);

  // Zoom handlers
  const handleWheel = useCallback((e: React.WheelEvent) => {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    const delta = e.deltaY > 0 ? -0.08 : 0.08;
    setZoom((prev) => Math.max(0.2, Math.min(3, prev + delta)));
  }, []);

  const handleZoomIn = () => setZoom((prev) => Math.min(3, prev + 0.15));
  const handleZoomOut = () => setZoom((prev) => Math.max(0.2, prev - 0.15));

  // Keyboard shortcuts
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "f" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        fitToScreen();
      }
      if (e.key === "=" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        handleZoomIn();
      }
      if (e.key === "-" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        handleZoomOut();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isOpen, onClose, fitToScreen]);

  // Get the table that a relation connects to for the FK info popup
  const relationInfo = useMemo(() => {
    if (!hoveredRelation) return null;
    const rel = relations.find((r) => r.constraint_name === hoveredRelation);
    if (!rel) return null;
    const fromTable = tables.find((t) => t.table === rel.from_table);
    const toTable = tables.find((t) => t.table === rel.to_table);
    return { rel, fromTable, toTable };
  }, [hoveredRelation, relations, tables]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 bg-[var(--bg-primary)] flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--bg-secondary)] shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-[var(--accent)]/10 text-[var(--accent)]">
            <Table2 className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-base font-bold text-[var(--text-primary)]">
              ER Diagram — {database}
            </h2>
            <p className="text-xs text-[var(--text-muted)]">
              {tables.length} table{tables.length !== 1 ? "s" : ""}
              {relations.length > 0 && ` · ${relations.length} relationship${relations.length !== 1 ? "s" : ""}`}
              · Drag tables · Scroll to zoom · Drag background to pan
            </p>
          </div>
        </div>
        <div className="flex items-center gap-1">
          <span className="text-xs text-[var(--text-muted)] mr-2">
            {Math.round(zoom * 100)}%
          </span>
          <button
            onClick={handleZoomOut}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
            title="Zoom Out (Ctrl+-)"
          >
            <ZoomOut className="w-4 h-4" />
          </button>
          <button
            onClick={handleZoomIn}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
            title="Zoom In (Ctrl+=)"
          >
            <ZoomIn className="w-4 h-4" />
          </button>
          <button
            onClick={fitToScreen}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
            title="Fit to Screen (Ctrl+F)"
          >
            <Maximize2 className="w-4 h-4" />
          </button>
          <div className="w-px h-5 mx-1 bg-[var(--border)]" />
          <button
            onClick={handleAutoLayout}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
            title="Auto Layout (BFS relationship-based)"
          >
            <Layers className="w-4 h-4" />
          </button>
          <button
            onClick={handleResetLayout}
            className="p-1.5 rounded-md hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
            title="Reset Grid Layout"
          >
            <Grid3X3 className="w-4 h-4" />
          </button>
          <button
            onClick={handleExportPng}
            disabled={exporting}
            className={`p-1.5 rounded-md transition-colors ${
              exporting
                ? "opacity-50 cursor-wait"
                : "hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            }`}
            title="Export as PNG"
          >
            {exporting ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <ImageDown className="w-4 h-4" />
            )}
          </button>
          <div className="w-px h-5 mx-1 bg-[var(--border)]" />
          <button
            onClick={onClose}
            className="p-1.5 rounded-md hover:bg-red-500/10 hover:text-red-500 text-[var(--text-muted)] transition-colors"
            title="Close (Esc)"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Loading */}
      {loading && (
        <div className="flex-1 flex items-center justify-center">
          <div className="flex flex-col items-center gap-3">
            <Loader2 className="w-8 h-8 animate-spin text-[var(--accent)]" />
            <p className="text-sm text-[var(--text-muted)]">Loading ER diagram...</p>
          </div>
        </div>
      )}

      {/* Error */}
      {error && !loading && (
        <div className="flex-1 flex items-center justify-center">
          <div className="flex flex-col items-center gap-3 text-center max-w-md">
            <AlertCircle className="w-10 h-10 text-red-400/70" />
            <p className="text-sm text-[var(--error)]">{error}</p>
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-md bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-sm hover:bg-[var(--border)] transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      )}

      {/* Canvas */}
      {!loading && !error && tables.length > 0 && (
        <div
          ref={containerRef}
          className="flex-1 overflow-hidden relative bg-[var(--bg-secondary)]"
          onMouseDown={handleBgMouseDown}
          onWheel={handleWheel}
          style={{ cursor: isPanning ? "grabbing" : dragging ? "grabbing" : "grab" }}
        >
          {/* Table Layer */}
          <div
            className="absolute inset-0"
            style={{
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
              transformOrigin: "0 0",
            }}
          >
            {tables.map((table) => {
              const pos = positions[table.table];
              if (!pos) return null;
              const isDraggingThis = dragging?.table === table.table;
              const isHoveredThis = hoveredTable === table.table;
              const isSelectedThis = selectedTable === table.table;
              const isCollapsedThis = !!collapsedTables[table.table];
              const tableRelations = relations.filter(
                (r) => r.from_table === table.table || r.to_table === table.table
              );
              const height = getTableHeight(table, isCollapsedThis);

              const dbColor = getDatabaseColor(table.database);
              const dbColorStr = `hsla(${dbColor.hue}, ${dbColor.sat}%, 50%, `;

              return (
                <div
                  key={table.table}
                  data-table-box
                  className={`absolute rounded-xl border-2 transition-shadow duration-200 select-none ${
                    isSelectedThis
                      ? "shadow-xl z-30"
                      : isHoveredThis
                      ? "shadow-lg z-20"
                      : "shadow-md hover:shadow-lg z-10"
                  } ${isDraggingThis ? "shadow-2xl z-50 cursor-grabbing" : "cursor-grab"}`}
                  style={{
                    left: pos.x,
                    top: pos.y,
                    width: TABLE_WIDTH,
                    height,
                    background: "var(--bg-primary)",
                    borderColor: isSelectedThis
                      ? dbColorStr + "0.8)"
                      : isHoveredThis
                      ? dbColorStr + "0.5)"
                      : dbColorStr + "0.2)",
                    boxShadow: isSelectedThis
                      ? `0 0 20px ${dbColorStr}0.15)`
                      : isHoveredThis
                      ? `0 0 12px ${dbColorStr}0.08)`
                      : "none",
                    transition: isDraggingThis ? "none" : "box-shadow 0.15s ease, border-color 0.15s ease",
                  }}
                  onMouseDown={(e) => handleTableMouseDown(e, table.table)}
                  onMouseEnter={() => setHoveredTable(table.table)}
                  onMouseLeave={() => setHoveredTable(null)}
                  onClick={() => setSelectedTable(selectedTable === table.table ? null : table.table)}
                >
                  {/* Table Header */}
                  <div
                    className={`flex items-center justify-between px-3 h-[42px] rounded-t-xl border-b`}
                    style={{
                      background: isSelectedThis
                        ? `${dbColorStr}0.15)`
                        : `${dbColorStr}0.06)`,
                      borderColor: isSelectedThis
                        ? `${dbColorStr}0.3)`
                        : `${dbColorStr}0.12)`,
                    }}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setCollapsedTables((prev) => ({
                            ...prev,
                            [table.table]: !prev[table.table],
                          }));
                        }}
                        className="p-0.5 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors shrink-0"
                        title={collapsedTables[table.table] ? "Expand columns" : "Collapse columns"}
                      >
                        <svg
                          className={`w-3.5 h-3.5 transition-transform duration-200 ${collapsedTables[table.table] ? "-rotate-90" : ""}`}
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2.5"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        >
                          <polyline points="6 9 12 15 18 9" />
                        </svg>
                      </button>
                      <div
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{ background: `${dbColorStr}0.8)` }}
                      />
                      <Table2 className={`w-4 h-4 shrink-0 ${
                        isSelectedThis ? "" : "opacity-60"
                      }`} style={{ color: `${dbColorStr}${isSelectedThis ? "1" : "0.7"})` }} />
                      <span className={`text-sm font-semibold truncate ${
                        isSelectedThis ? "text-[var(--text-primary)]" : "text-[var(--text-primary)]"
                      }`}>
                        {table.table}
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      {table.database !== database && (
                        <span className="text-[9px] text-[var(--text-muted)]/60 font-mono truncate max-w-[80px]">
                          {table.database}
                        </span>
                      )}
                      <span className="text-[10px] text-[var(--text-muted)] font-mono whitespace-nowrap">
                        {table.row_count.toLocaleString()} rows
                      </span>
                    </div>
                  </div>

                  {/* Columns */}
                  {collapsedTables[table.table] ? (
                    <div className="px-3 h-[2px]" />
                  ) : (
                  <div className="divide-y divide-[var(--border)]/30">
                    {table.columns.map((col) => {
                      const isFkCol = relations.some(
                        (r) => r.from_table === table.table && r.from_column === col.name
                      );
                      const isPkCol = col.is_primary_key;
                      const hasRelation = relations.some(
                        (r) => r.from_table === table.table && r.from_column === col.name
                      );
                      const relTarget = hasRelation
                        ? relations.find(
                            (r) => r.from_table === table.table && r.from_column === col.name
                          )
                        : null;

                      return (
                        <div
                          key={col.name}
                          className={`flex items-center gap-2 px-3 h-[32px] text-xs group transition-colors ${
                            isFkCol
                              ? "bg-[var(--accent)]/[0.03]"
                              : "hover:bg-[var(--bg-tertiary)]/50"
                          }`}
                        >
                          {isPkCol ? (
                            <Key className="w-3 h-3 text-yellow-500 shrink-0" aria-label="Primary Key" />
                          ) : isFkCol ? (
                            <Link2 className="w-3 h-3 text-purple-400 shrink-0" aria-label="Foreign Key" />
                          ) : (
                            <div className="w-3 h-3 shrink-0" />
                          )}
                          <span className="text-[var(--text-primary)] font-mono truncate flex-1">
                            {col.name}
                          </span>
                          <span className="text-[var(--text-muted)] text-[10px] font-mono hidden group-hover:inline truncate max-w-[100px]">
                            {col.column_type.split("(")[0]}
                          </span>
                          {relTarget && (
                            <span className="text-[10px] text-purple-400/70 font-mono truncate max-w-[80px] hidden group-hover:inline">
                              → {relTarget.to_table}
                            </span>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  )}

                  {/* Table Actions (shown on hover/select) */}
                  {(isHoveredThis || isSelectedThis) && (
                    <div className="absolute -top-3 right-3 flex items-center gap-1 z-40">
                      {onViewData && (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            onViewData(database, table.table);
                          }}
                          className="px-2 py-1 rounded-md bg-[var(--accent)] text-white text-[10px] font-medium hover:opacity-90 transition-opacity shadow-sm"
                        >
                          View Data
                        </button>
                      )}
                    </div>
                  )}

                  {/* Relationship count badge */}
                  {tableRelations.length > 0 && !isSelectedThis && !isHoveredThis && (
                    <div className="absolute -top-2 -right-2 w-5 h-5 rounded-full bg-[var(--accent)]/20 border border-[var(--accent)]/40 flex items-center justify-center">
                      <span className="text-[9px] font-bold text-[var(--accent)]">
                        {tableRelations.length}
                      </span>
                    </div>
                  )}
                </div>
              );
            })}

            {/* SVG Layer - FK Lines (placed inside the transformed container so they pan/zoom natively) */}
            <svg
              ref={svgRef}
              className="absolute inset-0 w-full h-full pointer-events-none"
              style={{ zIndex: 10, overflow: "visible" }}
            >
              <defs>
                <marker
                  id="arrowhead"
                  markerWidth="10"
                  markerHeight="7"
                  refX="10"
                  refY="3.5"
                  orient="auto"
                >
                  <polygon
                    points="0 0, 10 3.5, 0 7"
                    fill="currentColor"
                  />
                </marker>
              </defs>
              {relations.map((rel, i) => {
                  const key = rel.constraint_name || `rel-${i}`;
                  const isHovered = hoveredRelation === key;
                  const path = getRelationPath(rel);
                  if (!path) return null;

                  const relDbColor = getDatabaseColor(tables.find(t => t.table === rel.from_table)?.database || database);

                  const colorStr = `hsla(${relDbColor.hue}, ${relDbColor.sat}%, 50%, 1)`;
                  return (
                    <g
                      key={key}
                      style={{ color: colorStr }}
                      opacity={isHovered ? 0.9 : 0.5}
                    >
                      {/* Invisible wider path for hover detection */}
                      <path
                        d={path}
                        fill="none"
                        stroke="transparent"
                        strokeWidth={12}
                        className="pointer-events-auto cursor-pointer"
                        onMouseEnter={() => setHoveredRelation(key)}
                        onMouseLeave={() => setHoveredRelation(null)}
                      />
                      {/* Visible path */}
                      <path
                        d={path}
                        fill="none"
                        stroke={colorStr}
                        strokeWidth={isHovered ? 2.5 : 1.5}
                        strokeDasharray={isHovered ? "none" : "6 3"}
                        className="transition-all duration-200"
                        markerEnd="url(#arrowhead)"
                      />
                      {/* FK label */}
                      {isHovered && (() => {
                        const arrow = getArrowPosition(rel);
                        const fromTable_ = tables.find(t => t.table === rel.from_table);
                        if (!fromTable_) return null;
                        const fromPos_ = positions[rel.from_table];
                        if (!fromPos_) return null;
                        const fromRight = fromPos_.x + TABLE_WIDTH;
                        const fromMidY = fromPos_.y + getTableHeight(fromTable_) / 2;
                        return (
                          <text
                            x={(arrow.x + fromRight) / 2}
                            y={(arrow.y + fromMidY) / 2 - 8}
                            fill={colorStr}
                            fontSize="11"
                            textAnchor="middle"
                            className="select-none"
                          >
                            {rel.from_column} → {rel.to_table}.{rel.to_column}
                          </text>
                        );
                      })()}
                    </g>
                  );
                })}
            </svg>
          </div>

          {/* Relation Info Tooltip */}
          {hoveredRelation && relationInfo && (
            <div
              className="absolute bottom-4 left-1/2 -translate-x-1/2 z-50 px-4 py-3 rounded-xl bg-[var(--bg-primary)] border border-[var(--accent)]/40 shadow-2xl"
              style={{ maxWidth: 500 }}
            >
              <div className="flex items-center gap-3">
                <Link2 className="w-4 h-4 text-purple-400 shrink-0" />
                <div className="text-sm">
                  <span className="text-[var(--text-primary)] font-medium">
                    {relationInfo.rel.from_table}
                  </span>
                  <span className="text-[var(--text-muted)] mx-1.5">.</span>
                  <span className="text-purple-400 font-mono">{relationInfo.rel.from_column}</span>
                  <span className="text-[var(--text-muted)] mx-2">→</span>
                  <span className="text-[var(--text-primary)] font-medium">
                    {relationInfo.rel.to_table}
                  </span>
                  <span className="text-[var(--text-muted)] mx-1.5">.</span>
                  <span className="text-[var(--accent)] font-mono">{relationInfo.rel.to_column}</span>
                  {relationInfo.rel.constraint_name && (
                    <>
                      <span className="text-[var(--text-muted)] mx-2">·</span>
                      <span className="text-[var(--text-muted)] text-xs">
                        {relationInfo.rel.constraint_name}
                      </span>
                    </>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Minimap */}
          {showMinimap && tables.length > 0 && (() => {
            // Calculate content bounds
            const tableBounds = tables.map(t => {
              const p = positions[t.table];
              const h = getTableHeight(t);
              return p ? { x: p.x, y: p.y, w: TABLE_WIDTH, h } : null;
            }).filter(Boolean) as { x: number; y: number; w: number; h: number }[];

            if (tableBounds.length === 0) return null;

            const minX = Math.min(...tableBounds.map(b => b.x));
            const minY = Math.min(...tableBounds.map(b => b.y));
            const maxX = Math.max(...tableBounds.map(b => b.x + b.w));
            const maxY = Math.max(...tableBounds.map(b => b.y + b.h));
            const contentW = maxX - minX + 40;
            const contentH = maxY - minY + 40;

            const MINIMAP_W = 180;
            const MINIMAP_H = 120;
            const scale = Math.min(MINIMAP_W / contentW, MINIMAP_H / contentH, 0.3);
            const offsetX = (MINIMAP_W - contentW * scale) / 2 - minX * scale + 20 * scale;
            const offsetY = (MINIMAP_H - contentH * scale) / 2 - minY * scale + 20 * scale;

            // Viewport rectangle in content coordinates
            const container = containerRef.current;
            const vpX = -pan.x / zoom;
            const vpY = -pan.y / zoom;
            const vpW = container ? container.clientWidth / zoom : 500;
            const vpH = container ? container.clientHeight / zoom : 400;

            return (
              <div
                ref={minimapRef}
                className="absolute bottom-3 left-3 z-50 rounded-xl overflow-hidden border border-[var(--border)] shadow-2xl bg-[var(--bg-primary)]/95 backdrop-blur-sm cursor-pointer select-none"
                style={{ width: MINIMAP_W + 16, height: MINIMAP_H + 32 }}
              >
                {/* Minimap header */}
                <div className="flex items-center justify-between px-2 py-1 border-b border-[var(--border)]">
                  <span className="text-[10px] text-[var(--text-muted)] font-medium flex items-center gap-1">
                    <Map className="w-3 h-3" />
                    Overview
                  </span>
                  <button
                    onClick={(e) => { e.stopPropagation(); setShowMinimap(false); }}
                    className="p-0.5 rounded hover:bg-[var(--bg-tertiary)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
                  >
                    <Minimize2 className="w-3 h-3" />
                  </button>
                </div>
                {/* Minimap canvas */}
                <div
                  className="minimap-canvas relative"
                  style={{ width: MINIMAP_W, height: MINIMAP_H, margin: '2px 8px 6px 8px' }}
                  onMouseDown={(e) => {
                    e.stopPropagation();
                    e.preventDefault();
                    isDraggingMinimap.current = true;
                    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    const mx = e.clientX - rect.left;
                    const my = e.clientY - rect.top;
                    // Convert minimap coordinates to content coordinates
                    const targetX = (mx - offsetX) / scale;
                    const targetY = (my - offsetY) / scale;
                    const containerEl = containerRef.current;
                    if (containerEl) {
                      const newPanX = -(targetX - containerEl.clientWidth / (2 * zoom));
                      const newPanY = -(targetY - containerEl.clientHeight / (2 * zoom));
                      setPan({ x: newPanX, y: newPanY });
                    }
                  }}
                >
                  <svg
                    width={MINIMAP_W}
                    height={MINIMAP_H}
                    className="absolute inset-0"
                  >
                    {/* FK lines in minimap (colored by source database) */}
                    {relations.map((rel, i) => {
                      const fromPos = positions[rel.from_table];
                      const toPos = positions[rel.to_table];
                      if (!fromPos || !toPos) return null;
                      const fromTable = tables.find(t => t.table === rel.from_table);
                      const toTable = tables.find(t => t.table === rel.to_table);
                      const relDbColor = getDatabaseColor(fromTable?.database || database);
                      const fromH = fromTable ? getTableHeight(fromTable) : 0;
                      const toH = toTable ? getTableHeight(toTable) : 0;
                      if (!fromTable || !toTable) return null;
                      return (
                        <line
                          key={`mrel-${i}`}
                          x1={fromPos.x * scale + offsetX}
                          y1={(fromPos.y + fromH / 2) * scale + offsetY}
                          x2={toPos.x * scale + offsetX}
                          y2={(toPos.y + toH / 2) * scale + offsetY}
                          stroke={`hsla(${relDbColor.hue}, ${relDbColor.sat}%, 50%, 0.4)`}
                          strokeWidth={0.5}
                        />
                      );
                    })}
                    {/* Table rectangles */}
                    {tables.map((table) => {
                      const p = positions[table.table];
                      if (!p) return null;
                      const isColl = !!collapsedTables[table.table];
                      const h = getTableHeight(table, isColl);
                      const c = getDatabaseColor(table.database);
                      const isSel = selectedTable === table.table;
                      return (
                        <rect
                          key={`mm-${table.table}`}
                          x={p.x * scale + offsetX}
                          y={p.y * scale + offsetY}
                          width={TABLE_WIDTH * scale}
                          height={h * scale}
                          rx={2}
                          ry={2}
                          fill={`hsla(${c.hue}, ${c.sat}%, 50%, ${isSel ? 0.6 : 0.25})`}
                          stroke={`hsla(${c.hue}, ${c.sat}%, 50%, ${isSel ? 0.9 : 0.4})`}
                          strokeWidth={isSel ? 0.8 : 0.5}
                        />
                      );
                    })}
                    {/* Viewport rectangle */}
                    <rect
                      x={vpX * scale + offsetX}
                      y={vpY * scale + offsetY}
                      width={vpW * scale}
                      height={vpH * scale}
                      fill="none"
                      stroke="var(--accent)"
                      strokeWidth={1.5}
                      strokeOpacity={0.8}
                      rx={1}
                      ry={1}
                      className="pointer-events-none"
                    />
                  </svg>
                </div>
              </div>
            );
          })()}

          {/* Minimap toggle button (when hidden) */}
          {!showMinimap && (
            <button
              onClick={() => setShowMinimap(true)}
              className="absolute bottom-3 left-3 z-50 px-2.5 py-1.5 rounded-lg bg-[var(--bg-primary)]/80 backdrop-blur-sm border border-[var(--border)] shadow-lg text-xs text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--bg-primary)] transition-colors flex items-center gap-1.5"
            >
              <Map className="w-3.5 h-3.5" />
              Overview
            </button>
          )}

          {/* Bottom Legend */}
          <div className="absolute bottom-3 right-3 flex flex-col gap-2 z-40">
            {/* Controls Legend */}
            <div className="flex items-center gap-3 px-3 py-2 rounded-lg bg-[var(--bg-primary)]/80 backdrop-blur-sm border border-[var(--border)] text-xs text-[var(--text-muted)]">
              <span className="flex items-center gap-1">
                <Key className="w-3 h-3 text-yellow-500" /> PK
              </span>
              <span className="flex items-center gap-1">
                <Link2 className="w-3 h-3 text-purple-400" /> FK
              </span>
              <span className="text-[var(--border)]">|</span>
              <span className="flex items-center gap-1">
                <MousePointer2 className="w-3 h-3" /> Click
              </span>
              <span className="flex items-center gap-1">
                <Move className="w-3 h-3" /> Drag
              </span>
              <span className="flex items-center gap-1">
                <Layers className="w-3 h-3" /> Auto
              </span>
            </div>
            {/* Database Colors Legend */}
            {(tables.length > 0) && (() => {
              const dbSet = new Set(tables.map(t => t.database));
              const dbList = [...dbSet];
              if (dbList.length <= 1 && dbList[0] === database) return null;
              return (
                <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-[var(--bg-primary)]/80 backdrop-blur-sm border border-[var(--border)] text-[10px] text-[var(--text-muted)]">
                  <Palette className="w-3 h-3" />
                  {dbList.map((db) => {
                    const c = getDatabaseColor(db);
                    return (
                      <span key={db} className="flex items-center gap-1">
                        <span
                          className="w-2 h-2 rounded-full"
                          style={{ background: `hsla(${c.hue}, ${c.sat}%, 50%, 0.8)` }}
                        />
                        {db}
                      </span>
                    );
                  })}
                </div>
              );
            })()}
          </div>

          {/* Empty state */}
          {tables.length === 0 && !loading && (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="text-center">
                <Table2 className="w-12 h-12 mx-auto mb-3 text-[var(--text-muted)]/30" />
                <p className="text-sm text-[var(--text-muted)]">No tables found in {database}</p>
                <p className="text-xs text-[var(--text-muted)] mt-1">Cache the schema first from the sidebar</p>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Footer */}
      <div className="flex items-center justify-between px-5 py-2 border-t border-[var(--border)] bg-[var(--bg-secondary)] text-xs text-[var(--text-muted)] shrink-0">
        <span>
          {tables.length} tables · {relations.length} relationships
        </span>
        <span className="flex items-center gap-2">
          <kbd className="px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border)] text-[10px] font-mono">Ctrl+F</kbd>
          <span>Fit</span>
          <kbd className="px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border)] text-[10px] font-mono">Esc</kbd>
          <span>Close</span>
          <kbd className="px-1.5 py-0.5 rounded bg-[var(--bg-tertiary)] border border-[var(--border)] text-[10px] font-mono">Ctrl+Scroll</kbd>
          <span>Zoom</span>
        </span>
      </div>
    </div>
  );
}

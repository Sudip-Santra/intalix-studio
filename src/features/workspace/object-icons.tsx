import {
  Cog,
  Eye,
  Folder,
  FolderOpen,
  Layers,
  ListOrdered,
  SquareFunction,
  Table2,
  TableProperties,
  Globe,
  FileCode2,
  type LucideIcon,
} from "lucide-react";
import type { RelationKind, RoutineSummary } from "@/services/client";

export const RELATION_META: Record<RelationKind, { icon: LucideIcon; label: string; color: string }> = {
  table: { icon: Table2, label: "Table", color: "var(--syntax-type)" },
  partitioned_table: { icon: TableProperties, label: "Partitioned table", color: "var(--syntax-type)" },
  foreign_table: { icon: Globe, label: "Foreign table", color: "var(--syntax-type)" },
  view: { icon: Eye, label: "View", color: "var(--syntax-number)" },
  materialized_view: { icon: Layers, label: "Materialized view", color: "var(--syntax-number)" },
};

export const ROUTINE_ICON: Record<RoutineSummary["kind"], LucideIcon> = {
  function: SquareFunction,
  aggregate: SquareFunction,
  procedure: Cog,
};

export { Folder as SchemaIcon, FolderOpen as SchemaOpenIcon, ListOrdered as SequenceIcon, FileCode2 as QueryIcon };

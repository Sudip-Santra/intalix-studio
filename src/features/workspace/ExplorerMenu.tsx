import {
  ChevronsDownUp,
  ChevronsUpDown,
  Copy,
  Eye,
  EyeOff,
  FileCode2,
  Hash,
  ListOrdered,
  PanelTop,
  RotateCw,
  ScrollText,
  SquareTerminal,
  Table2,
} from "lucide-react";
import type { ColumnInfo, RelationKind, RoutineSummary, SchemaSummary, SequenceSummary } from "@/services/client";
import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import { copyToClipboard } from "@/lib/clipboard";
import { countRowsSql, createSchemaScript, selectScript, viewDataSql } from "./sql-scripts";
import { qualified, quoteIdent, useWorkspace } from "./workspace-context";

/** What was right-clicked in the explorer. */
export type MenuTarget =
  | { kind: "schema"; schema: SchemaSummary; nodeId: string; expanded: boolean }
  | { kind: "group"; schema: string; label: string; nodeId: string; expanded: boolean }
  | { kind: "relation"; schema: string; name: string; relationKind: RelationKind; nodeId: string; expanded: boolean }
  | { kind: "column"; schema: string; table: string; column: ColumnInfo }
  | { kind: "routine"; routine: RoutineSummary }
  | { kind: "sequence"; sequence: SequenceSummary };

type Props = {
  target: MenuTarget | null;
  showSystem: boolean;
  onToggle: (nodeId: string) => void;
  onRefreshSchema: (schema: string) => void;
  onRefreshRelation: (schema: string, name: string) => void;
  onRefreshAll: () => void;
  onCollapseAll: () => void;
  onToggleSystem: () => void;
};

export function ExplorerMenu(props: Props) {
  const { target } = props;
  return (
    <ContextMenuContent className="w-[240px]">
      {target === null && <BackgroundItems {...props} />}
      {target?.kind === "schema" && <SchemaItems {...props} target={target} />}
      {target?.kind === "group" && <GroupItems {...props} target={target} />}
      {target?.kind === "relation" && <RelationItems {...props} target={target} />}
      {target?.kind === "column" && <ColumnItems target={target} />}
      {target?.kind === "routine" && <RoutineItems routine={target.routine} />}
      {target?.kind === "sequence" && <SequenceItems sequence={target.sequence} />}
    </ContextMenuContent>
  );
}

function BackgroundItems({ showSystem, onRefreshAll, onCollapseAll, onToggleSystem }: Props) {
  return (
    <>
      <ContextMenuItem onSelect={onRefreshAll}>
        <RotateCw /> Refresh
      </ContextMenuItem>
      <ContextMenuItem onSelect={onCollapseAll}>
        <ChevronsDownUp /> Collapse all
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={onToggleSystem}>
        {showSystem ? <EyeOff /> : <Eye />} {showSystem ? "Hide system schemas" : "Show system schemas"}
      </ContextMenuItem>
    </>
  );
}

function ExpandItem({ expanded, onSelect }: { expanded: boolean; onSelect: () => void }) {
  return (
    <ContextMenuItem onSelect={onSelect} shortcut={expanded ? "←" : "→"}>
      {expanded ? <ChevronsDownUp /> : <ChevronsUpDown />} {expanded ? "Collapse" : "Expand"}
    </ContextMenuItem>
  );
}

function SchemaItems({
  target,
  onToggle,
  onRefreshSchema,
}: { target: Extract<MenuTarget, { kind: "schema" }> } & Props) {
  const { openQuery } = useWorkspace();
  const { schema } = target;
  return (
    <>
      <ContextMenuLabel>{schema.name}</ContextMenuLabel>
      <ContextMenuItem onSelect={() => openQuery(`-- Schema: ${schema.name}\n`, schema.name)}>
        <SquareTerminal /> Query tool
      </ContextMenuItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <ScrollText /> Scripts
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuItem onSelect={() => openQuery(createSchemaScript(schema), `CREATE ${schema.name}`)}>
            CREATE script
          </ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSeparator />
      <ExpandItem expanded={target.expanded} onSelect={() => onToggle(target.nodeId)} />
      <ContextMenuItem onSelect={() => onRefreshSchema(schema.name)}>
        <RotateCw /> Refresh
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void copyToClipboard(schema.name)}>
        <Copy /> Copy name
      </ContextMenuItem>
    </>
  );
}

function GroupItems({ target, onToggle, onRefreshSchema }: { target: Extract<MenuTarget, { kind: "group" }> } & Props) {
  return (
    <>
      <ContextMenuLabel>
        {target.schema} · {target.label}
      </ContextMenuLabel>
      <ExpandItem expanded={target.expanded} onSelect={() => onToggle(target.nodeId)} />
      <ContextMenuItem onSelect={() => onRefreshSchema(target.schema)}>
        <RotateCw /> Refresh
      </ContextMenuItem>
    </>
  );
}

function RelationItems({
  target,
  onToggle,
  onRefreshRelation,
}: { target: Extract<MenuTarget, { kind: "relation" }> } & Props) {
  const { openRelation, openQuery, loadRelation } = useWorkspace();
  const { schema, name, relationKind } = target;
  const isView = relationKind === "view" || relationKind === "materialized_view";

  /** Needs column/key details first; falls back to a plain query if they can't be loaded. */
  const withDetail = (build: (d: Awaited<ReturnType<typeof loadRelation>> | null) => [string, string, boolean]) => {
    loadRelation(schema, name)
      .then((d) => build(d))
      .catch(() => build(null))
      .then(([sql, title, run]) => openQuery(sql, title, { run }));
  };

  const view = (mode: "first" | "last" | "all" | "filtered") =>
    withDetail((d) => [viewDataSql(schema, name, d, mode), name, mode !== "filtered"]);

  return (
    <>
      <ContextMenuLabel>
        {schema}.{name}
      </ContextMenuLabel>
      <ContextMenuItem onSelect={() => openRelation(schema, name, relationKind)} shortcut="Enter">
        <PanelTop /> Open
      </ContextMenuItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <Table2 /> View data
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuItem onSelect={() => view("first")}>First 100 rows</ContextMenuItem>
          <ContextMenuItem onSelect={() => view("last")}>Last 100 rows</ContextMenuItem>
          <ContextMenuItem onSelect={() => view("all")}>All rows</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => view("filtered")}>Filtered rows…</ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuItem onSelect={() => openQuery(countRowsSql(schema, name), `count ${name}`, { run: true })}>
        <Hash /> Count rows
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => openQuery(`SELECT *\nFROM ${qualified(schema, name)}\nLIMIT 100;`, name)}>
        <SquareTerminal /> Query tool
      </ContextMenuItem>
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <ScrollText /> Scripts
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuItem
            onSelect={() =>
              withDetail((d) => [
                d ? selectScript(schema, name, d) : `SELECT *\nFROM ${qualified(schema, name)}\nLIMIT 100;`,
                `SELECT ${name}`,
                false,
              ])
            }
          >
            SELECT script
          </ContextMenuItem>
          <ContextMenuItem
            onSelect={() =>
              withDetail((d) => [
                d?.ddl ?? `-- Could not load the definition of ${schema}.${name}\n`,
                `CREATE ${name}`,
                false,
              ])
            }
          >
            CREATE script
          </ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSeparator />
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <FileCode2 /> Properties
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuItem onSelect={() => openRelation(schema, name, relationKind, "columns")}>
            Columns
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => openRelation(schema, name, relationKind, "data")}>
            Data preview
          </ContextMenuItem>
          {!isView && (
            <>
              <ContextMenuItem onSelect={() => openRelation(schema, name, relationKind, "indexes")}>
                Indexes
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => openRelation(schema, name, relationKind, "constraints")}>
                Constraints
              </ContextMenuItem>
            </>
          )}
          <ContextMenuItem onSelect={() => openRelation(schema, name, relationKind, "ddl")}>
            {isView ? "Definition" : "DDL"}
          </ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ExpandItem expanded={target.expanded} onSelect={() => onToggle(target.nodeId)} />
      <ContextMenuItem onSelect={() => onRefreshRelation(schema, name)}>
        <RotateCw /> Refresh
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <Copy /> Copy
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          <ContextMenuItem onSelect={() => void copyToClipboard(name)}>Name</ContextMenuItem>
          <ContextMenuItem onSelect={() => void copyToClipboard(qualified(schema, name))}>
            Qualified name
          </ContextMenuItem>
          <ContextMenuItem
            onSelect={() =>
              void loadRelation(schema, name)
                .then((d) => copyToClipboard(d.columns.map((c) => quoteIdent(c.name)).join(", ")))
                .catch(() => undefined)
            }
          >
            Column list
          </ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
    </>
  );
}

function ColumnItems({ target }: { target: Extract<MenuTarget, { kind: "column" }> }) {
  const { openQuery } = useWorkspace();
  const { schema, table, column } = target;
  const col = quoteIdent(column.name);
  return (
    <>
      <ContextMenuLabel>
        {table}.{column.name} · {column.dataType}
      </ContextMenuLabel>
      <ContextMenuItem
        onSelect={() =>
          openQuery(`SELECT ${col}\nFROM ${qualified(schema, table)}\nLIMIT 100;`, `${table}.${column.name}`, {
            run: true,
          })
        }
      >
        <Table2 /> View values
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void copyToClipboard(column.name)}>
        <Copy /> Copy name
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void copyToClipboard(`${qualified(schema, table)}.${col}`)}>
        <Copy /> Copy qualified name
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void copyToClipboard(column.dataType)}>
        <Copy /> Copy data type
      </ContextMenuItem>
    </>
  );
}

function RoutineItems({ routine }: { routine: RoutineSummary }) {
  const { openQuery } = useWorkspace();
  const name = qualified(routine.schema, routine.name);
  const signature = `${name}(${routine.arguments})`;
  return (
    <>
      <ContextMenuLabel>{signature}</ContextMenuLabel>
      {routine.kind !== "procedure" && (
        <ContextMenuItem
          onSelect={() =>
            openQuery(`-- Arguments: ${routine.arguments || "none"}\nSELECT *\nFROM ${name}();`, routine.name)
          }
        >
          <ScrollText /> SELECT script
        </ContextMenuItem>
      )}
      <ContextMenuItem onSelect={() => void copyToClipboard(routine.name)}>
        <Copy /> Copy name
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void copyToClipboard(signature)}>
        <Copy /> Copy signature
      </ContextMenuItem>
    </>
  );
}

function SequenceItems({ sequence }: { sequence: SequenceSummary }) {
  const { openQuery } = useWorkspace();
  const name = qualified(sequence.schema, sequence.name);
  return (
    <>
      <ContextMenuLabel>{name}</ContextMenuLabel>
      <ContextMenuItem
        onSelect={() => openQuery(`SELECT last_value, is_called\nFROM ${name};`, sequence.name, { run: true })}
      >
        <ListOrdered /> Current value
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void copyToClipboard(sequence.name)}>
        <Copy /> Copy name
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void copyToClipboard(name)}>
        <Copy /> Copy qualified name
      </ContextMenuItem>
    </>
  );
}

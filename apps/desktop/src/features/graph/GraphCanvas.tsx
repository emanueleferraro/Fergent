/**
 * The React Flow canvas (GRAPH.md §8): pan/zoom/drag, node focus, and
 * open-note on double-click. Rendering is controlled here; positions come from
 * the layout module and every interaction is reported to the container.
 */
import { useCallback, useEffect, useMemo, type ReactElement } from 'react';
import ReactFlow, {
  Controls,
  MiniMap,
  useEdgesState,
  useNodesState,
  type Edge,
  type Node,
  type NodeMouseHandler,
} from 'reactflow';
import 'reactflow/dist/style.css';

import type { PositionedNode } from './layout';
import { nodeColorClass, type GraphEdge } from './model';
import { graphNodeTypes, type GraphNodeData } from './GraphNodeCard';

/** Props for `GraphCanvas`. */
export interface GraphCanvasProps {
  nodes: PositionedNode[];
  edges: GraphEdge[];
  /** The focused node, if any. */
  focusId: string | null;
  /** Node ids matched by the in-graph search. */
  matchIds: ReadonlySet<string>;
  /** Focus a node (or clear the focus with `null`). */
  onFocus: (id: string | null) => void;
  /** Open a node's note. */
  onOpen: (id: string) => void;
}

/** The React Flow canvas. */
export function GraphCanvas({
  nodes,
  edges,
  focusId,
  matchIds,
  onFocus,
  onOpen,
}: GraphCanvasProps): ReactElement {
  const neighbors = useMemo(() => {
    const around = new Set<string>();
    if (focusId) {
      around.add(focusId);
      for (const edge of edges) {
        if (edge.source === focusId) {
          around.add(edge.target);
        }
        if (edge.target === focusId) {
          around.add(edge.source);
        }
      }
    }
    return around;
  }, [edges, focusId]);

  const flowNodes = useMemo<Node<GraphNodeData>[]>(
    () =>
      nodes.map((node) => {
        let state = '';
        if (focusId && node.id === focusId) {
          state = 'is-focused';
        } else if (matchIds.has(node.id)) {
          state = 'is-match';
        } else if (focusId && !neighbors.has(node.id)) {
          state = 'is-dimmed';
        }
        return {
          id: node.id,
          type: 'graph',
          position: { x: node.x, y: node.y },
          data: { label: node.label, colorClass: nodeColorClass(node), state },
        };
      }),
    [nodes, focusId, matchIds, neighbors],
  );

  const flowEdges = useMemo<Edge[]>(
    () =>
      edges.map((edge) => {
        let className = '';
        if (focusId) {
          className =
            edge.source === focusId || edge.target === focusId ? 'is-focused' : 'is-dimmed';
        }
        return { id: edge.id, source: edge.source, target: edge.target, className };
      }),
    [edges, focusId],
  );

  const [elements, setElements, onNodesChange] = useNodesState<GraphNodeData>(flowNodes);
  const [links, setLinks, onEdgesChange] = useEdgesState(flowEdges);

  useEffect(() => {
    setElements(flowNodes);
  }, [flowNodes, setElements]);

  useEffect(() => {
    setLinks(flowEdges);
  }, [flowEdges, setLinks]);

  const onNodeClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      onFocus(node.id);
    },
    [onFocus],
  );

  const onNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      onOpen(node.id);
    },
    [onOpen],
  );

  const onPaneClick = useCallback(() => {
    onFocus(null);
  }, [onFocus]);

  return (
    <div className="graph-canvas">
      <ReactFlow
        nodes={elements}
        edges={links}
        nodeTypes={graphNodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        onNodeDoubleClick={onNodeDoubleClick}
        onPaneClick={onPaneClick}
        nodesConnectable={false}
        edgesFocusable={false}
        minZoom={0.1}
        maxZoom={2}
        fitView
      >
        <Controls showInteractive={false} />
        <MiniMap pannable zoomable />
      </ReactFlow>
    </div>
  );
}

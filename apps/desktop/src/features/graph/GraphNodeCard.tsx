/**
 * The custom React Flow node used by the graph view. The `state` field carries
 * the focus/dim/match class so the styling stays in `style/components.css`.
 */
import type { ReactElement } from 'react';
import type { NodeProps } from 'reactflow';

/** Data attached to every graph node. */
export interface GraphNodeData {
  label: string;
  colorClass: string;
  state: string;
}

function GraphNodeCard({ data }: NodeProps<GraphNodeData>): ReactElement {
  const classes = ['graph-node', `graph-node--${data.colorClass}`];
  if (data.state.length > 0) {
    classes.push(data.state);
  }
  return (
    <div className={classes.join(' ')}>
      <span className="graph-node__dot" aria-hidden="true" />
      <span className="graph-node__label">{data.label}</span>
    </div>
  );
}

/** The node types registry. Defined once so React Flow does not re-mount nodes. */
export const graphNodeTypes = { graph: GraphNodeCard };

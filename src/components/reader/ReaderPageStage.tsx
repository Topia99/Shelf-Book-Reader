import { isTouchDevice } from "../../platform";
import type { ReaderController, OutlineNode } from "./usePdfReaderController";

interface Props {
  controller: ReaderController;
  classNames?: {
    body?: string;
    outline?: string;
    container?: string;
    stage?: string;
    loading?: string;
    toast?: string;
  };
}

function cx(base: string, extra?: string) {
  return extra ? `${base} ${extra}` : base;
}

export default function ReaderPageStage({ controller, classNames }: Props) {
  const {
    containerRef,
    doc,
    finishTouchPointer,
    gotoPage,
    handleTouchPointerCancel,
    handleTouchPointerDown,
    handleTouchPointerMove,
    lookupSelectionWord,
    onContainerClick,
    onContainerMouseDown,
    outline,
    page,
    showOutline,
    text1Ref,
    text2Ref,
    toast,
    wrap2Ref,
    canvas1Ref,
    canvas2Ref,
  } = controller;

  return (
    <div className={cx("reader-body", classNames?.body)}>
      {showOutline && (
        <aside className={cx("outline-panel", classNames?.outline)}>
          <OutlineTree nodes={outline} onGoto={gotoPage} current={page} />
        </aside>
      )}
      <div
        className={cx("page-container", classNames?.container)}
        ref={containerRef}
        onDoubleClick={lookupSelectionWord}
        onMouseDown={isTouchDevice ? undefined : onContainerMouseDown}
        onClick={isTouchDevice ? undefined : onContainerClick}
        onPointerDown={handleTouchPointerDown}
        onPointerMove={handleTouchPointerMove}
        onPointerUp={finishTouchPointer}
        onPointerCancel={handleTouchPointerCancel}
      >
        <div className={cx("page-stage", classNames?.stage)}>
          <div className="page-wrap">
            <canvas ref={canvas1Ref} />
            <div className="textLayer" lang="en" ref={text1Ref} />
          </div>
          <div className="page-wrap" ref={wrap2Ref} style={{ display: "none" }}>
            <canvas ref={canvas2Ref} />
            <div className="textLayer" lang="en" ref={text2Ref} />
          </div>
        </div>
        {!doc && <div className={cx("loading-hint", classNames?.loading)}>正在打开…</div>}
        {toast && <div className={cx("reader-toast", classNames?.toast)}>{toast}</div>}
      </div>
    </div>
  );
}

function OutlineTree({
  nodes,
  onGoto,
  current,
  depth = 0,
}: {
  nodes: OutlineNode[];
  onGoto: (p: number) => void;
  current: number;
  depth?: number;
}) {
  return (
    <ul className="outline-list" style={{ paddingLeft: depth === 0 ? 0 : 14 }}>
      {nodes.map((node, index) => (
        <li key={index}>
          <button
            className={"outline-item" + (node.page === current ? " active" : "")}
            disabled={node.page === null}
            onClick={() => node.page !== null && onGoto(node.page)}
          >
            {node.title}
          </button>
          {node.children.length > 0 && (
            <OutlineTree
              nodes={node.children}
              onGoto={onGoto}
              current={current}
              depth={depth + 1}
            />
          )}
        </li>
      ))}
    </ul>
  );
}

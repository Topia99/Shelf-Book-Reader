import { isTouchDevice } from "../../platform";
import WordPopup from "../WordPopup";
import ReaderPageStage from "./ReaderPageStage";
import type { ReaderController } from "./usePdfReaderController";

interface Props {
  bookTitle: string;
  controller: ReaderController;
}

export default function ReaderDesktopShell({ bookTitle, controller }: Props) {
  const {
    displayScale,
    goBack,
    numPages,
    outline,
    pageInput,
    popup,
    setPageInput,
    setPopup,
    setShowOutline,
    setTwoPage,
    setZoom,
    showOutline,
    submitPageInput,
    toolbarHidden,
    twoPage,
    zoom,
    zoomBy,
  } = controller;

  return (
    <div className={"reader" + (isTouchDevice && toolbarHidden ? " toolbar-hidden" : "")}>
      <header className="reader-toolbar">
        <button className="btn" onClick={goBack} title="返回书库（Esc）">
          ← 书库
        </button>
        <button
          className={"btn" + (showOutline ? " active" : "")}
          onClick={() => setShowOutline((value) => !value)}
          disabled={outline.length === 0}
          title={outline.length === 0 ? "此 PDF 没有目录" : "目录"}
        >
          目录
        </button>
        <span className="reader-title">{bookTitle}</span>
        <div className="toolbar-group">
          <input
            className="page-input"
            value={pageInput}
            onChange={(e) => setPageInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submitPageInput()}
            onBlur={submitPageInput}
          />
          <span className="page-total">/ {numPages || "…"}</span>
        </div>
        <div className="toolbar-group">
          <button
            className={"btn" + (zoom === "fit-width" ? " active" : "")}
            onClick={() => setZoom("fit-width")}
          >
            适合宽度
          </button>
          <button
            className={"btn" + (zoom === "fit-page" ? " active" : "")}
            onClick={() => setZoom("fit-page")}
          >
            适合整页
          </button>
          <button className="btn" onClick={() => zoomBy(1 / 1.2)}>
            −
          </button>
          <span className="zoom-label">{Math.round(displayScale * 100)}%</span>
          <button className="btn" onClick={() => zoomBy(1.2)}>
            ＋
          </button>
        </div>
        <button className={"btn" + (twoPage ? " active" : "")} onClick={() => setTwoPage((value) => !value)}>
          {twoPage ? "双页" : "单页"}
        </button>
      </header>

      <ReaderPageStage controller={controller} />

      {popup && (
        <WordPopup
          result={popup.result}
          anchor={popup.anchor}
          onClose={() => setPopup(null)}
        />
      )}
    </div>
  );
}

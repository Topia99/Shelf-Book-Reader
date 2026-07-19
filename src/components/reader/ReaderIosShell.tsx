import { useEffect, useState } from "react";
import WordPopup from "../WordPopup";
import ReaderPageStage from "./ReaderPageStage";
import ReaderThumbnailStrip from "./ReaderThumbnailStrip";
import type { ReaderController } from "./usePdfReaderController";

interface Props {
  bookTitle: string;
  controller: ReaderController;
}

export default function ReaderIosShell({ bookTitle, controller }: Props) {
  const {
    doc,
    goBack,
    gotoPage,
    numPages,
    outline,
    page,
    popup,
    setPopup,
    setShowOutline,
    setZoom,
    showOutline,
    toolbarHidden,
    zoom,
  } = controller;
  const [badgeVisible, setBadgeVisible] = useState(true);

  useEffect(() => {
    if (!toolbarHidden) {
      setBadgeVisible(true);
      return;
    }
    setBadgeVisible(true);
    const timer = window.setTimeout(() => setBadgeVisible(false), 1400);
    return () => window.clearTimeout(timer);
  }, [page, toolbarHidden]);

  return (
    <div
      className={
        "reader-ios-shell" +
        (toolbarHidden ? " reader-ios-shell-hidden" : "") +
        (showOutline ? " reader-ios-shell-outline-open" : "")
      }
    >
      <ReaderPageStage
        controller={controller}
        classNames={{
          body: "reader-ios-body",
          outline: "reader-ios-outline-panel",
          container: "reader-ios-page-container",
          stage: "reader-ios-page-stage",
          loading: "reader-ios-loading-hint",
          toast: "reader-ios-toast",
        }}
      />

      <div className="reader-ios-chrome" aria-hidden={toolbarHidden}>
        <div className="reader-ios-toolbar reader-ios-toolbar-left">
          <button
            className="reader-ios-pill-button"
            aria-label={`返回书库：${bookTitle}`}
            title={`返回书库：${bookTitle}`}
            onClick={goBack}
          >
            <ChevronLeftIcon />
          </button>
          <button
            className={"reader-ios-pill-button" + (showOutline ? " active" : "")}
            aria-label={outline.length === 0 ? "此 PDF 没有目录" : "目录"}
            title={outline.length === 0 ? "此 PDF 没有目录" : "目录"}
            onClick={() => setShowOutline((value) => !value)}
            disabled={outline.length === 0}
          >
            <ListIcon />
          </button>
        </div>

        <div className="reader-ios-toolbar reader-ios-toolbar-right">
          <button
            className={"reader-ios-pill-button reader-ios-pill-label" + (zoom === "fit-width" ? " active" : "")}
            onClick={() => setZoom("fit-width")}
            aria-pressed={zoom === "fit-width"}
          >
            适宽
          </button>
          <button
            className={"reader-ios-pill-button reader-ios-pill-label" + (zoom === "fit-page" ? " active" : "")}
            onClick={() => setZoom("fit-page")}
            aria-pressed={zoom === "fit-page"}
          >
            整页
          </button>
        </div>

        {/* 底部缩略页胶片：位于 chrome 内，随工具栏一起显隐（验收 4.5） */}
        <ReaderThumbnailStrip doc={doc} page={page} numPages={numPages} onJump={gotoPage} />
      </div>

      <div
        className={"reader-ios-badge" + (!toolbarHidden || badgeVisible ? " is-visible" : "")}
        aria-live="polite"
      >
        {page}
        <span className="reader-ios-badge-separator">/</span>
        {numPages || "…"}
      </div>

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

function ChevronLeftIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M12.5 4.5 7 10l5.5 5.5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.9"
      />
    </svg>
  );
}

function ListIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M4.25 5.25h11.5M4.25 10h11.5M4.25 14.75h11.5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.8"
      />
      <circle cx="4.25" cy="5.25" r="0.9" fill="currentColor" />
      <circle cx="4.25" cy="10" r="0.9" fill="currentColor" />
      <circle cx="4.25" cy="14.75" r="0.9" fill="currentColor" />
    </svg>
  );
}

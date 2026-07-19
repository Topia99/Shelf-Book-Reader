import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Missing #root mount element");
}

const root = ReactDOM.createRoot(rootElement);

/** 首帧渲染成功后置 true：此后运行期错误只记日志，不再整页接管 */
let appMounted = false;

/** pdf.js 取消/销毁类的良性竞态（如 StrictMode 双挂载清理），直接忽略 */
const BENIGN_ERROR = /Transport destroyed|Rendering cancelled|AbortException|Worker was destroyed/i;

function renderFatal(message: string) {
  root.render(
    <div
      style={{
        minHeight: "100vh",
        padding: "24px",
        background: "#fff",
        color: "#7a1f1f",
        fontFamily: "-apple-system, BlinkMacSystemFont, sans-serif",
        whiteSpace: "pre-wrap",
      }}
    >
      <h1 style={{ margin: "0 0 12px", fontSize: "20px" }}>App startup failed</h1>
      <div>{message}</div>
    </div>
  );
}

/**
 * 真机白屏排查用的启动错误面板：只在首帧渲染前的窗口期接管页面；
 * 应用跑起来之后的错误交给 console（避免任何游离异步错误“灭屏”）。
 */
function handleGlobalError(detail: string) {
  if (BENIGN_ERROR.test(detail)) {
    console.warn("[ignored benign]", detail);
    return;
  }
  if (appMounted) {
    console.error("[runtime error]", detail);
    return;
  }
  renderFatal(detail);
}

window.addEventListener("error", (event) => {
  const detail =
    event.error instanceof Error
      ? `${event.error.name}: ${event.error.message}\n${event.error.stack ?? ""}`
      : event.message;
  handleGlobalError(detail || "Unknown startup error");
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  const detail =
    reason instanceof Error
      ? `${reason.name}: ${reason.message}\n${reason.stack ?? ""}`
      : String(reason ?? "Unhandled promise rejection");
  handleGlobalError(detail);
});

try {
  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
  // 首帧提交后关闭“启动失败”接管窗口
  requestAnimationFrame(() => {
    appMounted = true;
  });
} catch (error) {
  const detail =
    error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ""}` : String(error);
  renderFatal(detail);
}

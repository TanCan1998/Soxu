import { render } from "preact";
import "./index.css";

function App() {
  return (
    <main class="min-h-dvh flex flex-col justify-between p-4 max-w-md mx-auto">
      <header class="pt-2 pb-4 border-b border-slate-800">
        <p class="text-xs font-medium tracking-wider uppercase text-indigo-400">
          Soxu Remote · Protocol v1
        </p>
        <h1 class="text-xl font-semibold mt-1">多 Agent 移动遥控端</h1>
      </header>

      <section class="my-auto py-8 text-sm text-slate-400 leading-relaxed">
        移动伴侣工程骨架已就绪。完整界面与触控审批卡片将在 M0.5 设计系统定稿后按统一 DTCG 设计令牌实现。
      </section>

      <footer class="pt-3 border-t border-slate-800 text-xs text-slate-500">
        Soxu Daemon Embedded PWA
      </footer>
    </main>
  );
}

const root = document.getElementById("app");
if (root) {
  render(<App />, root);
}

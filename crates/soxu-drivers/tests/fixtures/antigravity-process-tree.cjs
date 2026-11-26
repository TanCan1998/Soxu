const { spawn } = require("node:child_process");
const assert = require("node:assert/strict");
const fs = require("node:fs");

if (process.argv[2] === "check") {
  for (const pid of process.argv.slice(3).map(Number)) {
    try {
      process.kill(pid, 0);
    } catch (error) {
      if (error.code === "ESRCH") continue;
      throw error;
    }
    if (process.platform === "linux") {
      const stat = fs.readFileSync("/proc/" + pid + "/stat", "utf8");
      if (stat.slice(stat.lastIndexOf(")") + 2).startsWith("Z ")) continue;
    }
    throw new Error("进程仍存活: " + pid);
  }
} else {
  assert.equal(process.env.SO_XU_HOST_SECRET, undefined);
  assert.equal(process.env.NODE_OPTIONS, undefined);
  const hook = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
    stdio: "ignore",
    windowsHide: true,
  });
  hook.once("spawn", () => {
    const text = process.pid + " " + hook.pid;
    fs.writeFileSync(process.env.SOXU_TEST_TREE_PIDS, text);
    console.log(JSON.stringify({
      event: "step_update",
      step_update: { step_type: "agent_response", text_delta: text },
    }));
  });
}

import { existsSync, rmSync } from "node:fs";
import http from "node:http";
import { dirname, resolve } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import net from "node:net";

const cwd = process.cwd();
const task = process.argv[2] || "build:desktop";
const desktopDevHost = "127.0.0.1";
const desktopDevPort = 3005;
const desktopNextPort = 3006;
const desktopDevWaitTimeoutMs = 10_000;
const desktopDevWaitIntervalMs = 500;
const desktopDevWarmupTimeoutMs = 120_000;
const candidates = [
  cwd,
  resolve(cwd, "apps"),
  resolve(cwd, "..", "apps"),
  resolve(cwd, "..", "..", "apps"),
  resolve(cwd, ".."),
  resolve(cwd, "..", ".."),
];

function hasFrontendPackage(dir) {
  return existsSync(resolve(dir, "package.json"));
}

function hasBuiltFrontendDist(dir) {
  return existsSync(resolve(dir, "out", "index.html"));
}

function canConnect(host, port, timeoutMs = 1000) {
  return new Promise((resolvePromise) => {
    const socket = new net.Socket();
    let settled = false;

    const finish = (result) => {
      if (settled) {
        return;
      }
      settled = true;
      socket.destroy();
      resolvePromise(result);
    };

    socket.setTimeout(timeoutMs);
    socket.once("connect", () => finish(true));
    socket.once("timeout", () => finish(false));
    socket.once("error", () => finish(false));
    socket.connect(port, host);
  });
}

async function hasReusableDesktopDevServer() {
  const reachable = await canConnect(desktopDevHost, desktopDevPort);
  if (!reachable) {
    return false;
  }

  try {
    const response = await fetch(`http://${desktopDevHost}:${desktopDevPort}/startup.html`, {
      signal: AbortSignal.timeout(1500),
    });
    return response.ok;
  } catch {
    return false;
  }
}

function getDesktopDevLockPath(dir) {
  return resolve(dir, ".next", "dev", "lock");
}

function sleep(ms) {
  return new Promise((resolvePromise) => {
    setTimeout(resolvePromise, ms);
  });
}

async function waitForDesktopDevPortsToClose(timeoutMs = 5_000) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() <= deadline) {
    const proxyClosed = !(await canConnect(desktopDevHost, desktopDevPort, 300));
    const nextClosed = !(await canConnect(desktopDevHost, desktopNextPort, 300));
    if (proxyClosed && nextClosed) {
      return true;
    }

    if (Date.now() >= deadline) {
      return false;
    }

    await sleep(250);
  }

  return false;
}

async function waitForReusableDesktopDevServer(timeoutMs = desktopDevWaitTimeoutMs) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() <= deadline) {
    if (await hasReusableDesktopDevServer()) {
      return true;
    }

    if (Date.now() >= deadline) {
      return false;
    }

    await sleep(desktopDevWaitIntervalMs);
  }

  return false;
}

async function fetchDesktopDevPath(path, timeoutMs, port = desktopDevPort) {
  const response = await fetch(`http://${desktopDevHost}:${port}${path}`, {
    signal: AbortSignal.timeout(timeoutMs),
  });
  await response.arrayBuffer();
  return response.ok;
}

async function warmupDesktopDevServer() {
  const deadline = Date.now() + desktopDevWarmupTimeoutMs;
  while (Date.now() <= deadline) {
    try {
      if (await fetchDesktopDevPath("/startup.html", 1500, desktopNextPort)) {
        break;
      }
    } catch {
      // Keep waiting until Next starts serving static files.
    }

    if (Date.now() >= deadline) {
      console.error(`等待前端开发服务就绪超时: http://${desktopDevHost}:${desktopNextPort}/startup.html`);
      process.exit(1);
    }

    await sleep(desktopDevWaitIntervalMs);
  }

  console.log(`前端静态启动页已就绪: http://${desktopDevHost}:${desktopNextPort}/startup.html`);
  try {
    const warmed = await fetchDesktopDevPath("/", desktopDevWarmupTimeoutMs, desktopNextPort);
    if (!warmed) {
      console.error(`前端首页预热失败: http://${desktopDevHost}:${desktopNextPort}/`);
      process.exit(1);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`前端首页预热失败: ${message}`);
    process.exit(1);
  }
  console.log(`前端首页已预热完成: http://${desktopDevHost}:${desktopNextPort}/`);
}

function listDesktopDevListenerPids(port = desktopDevPort) {
  const result = spawnSync("netstat", ["-ano", "-p", "tcp"], {
    encoding: "utf8",
  });

  if (result.error || result.status !== 0) {
    return [];
  }

  const expectedAddress = `${desktopDevHost}:${port}`;
  const pids = new Set();

  for (const line of result.stdout.split(/\r?\n/)) {
    if (!line.includes(expectedAddress) || !/\bLISTENING\b/i.test(line)) {
      continue;
    }

    const match = line.trim().match(/(\d+)$/);
    if (!match) {
      continue;
    }

    pids.add(Number.parseInt(match[1], 10));
  }

  return [...pids];
}

function getWindowsProcessInfo(pid) {
  const command = [
    `$process = Get-CimInstance Win32_Process -Filter "ProcessId = ${pid}" -ErrorAction SilentlyContinue`,
    'if ($process) { $process | Select-Object ProcessId, ParentProcessId, CommandLine | ConvertTo-Json -Compress }',
  ].join("; ");

  const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", command], {
    encoding: "utf8",
  });

  if (result.error || result.status !== 0) {
    return null;
  }

  const rawOutput = result.stdout.trim();
  if (!rawOutput) {
    return null;
  }

  try {
    return JSON.parse(rawOutput);
  } catch {
    return null;
  }
}

function isDesktopDevProcess(pid, port = desktopDevPort) {
  let currentPid = pid;

  for (let index = 0; index < 4 && currentPid; index += 1) {
    const processInfo = getWindowsProcessInfo(currentPid);
    if (!processInfo?.CommandLine) {
      break;
    }

    const normalizedCommandLine = processInfo.CommandLine.toLowerCase();
    const isNextProcess =
      normalizedCommandLine.includes("next dev") ||
      normalizedCommandLine.includes("\\next\\dist\\bin\\next") ||
      normalizedCommandLine.includes("start-server.js");
    const isDevProxyProcess =
      normalizedCommandLine.includes("before-build.mjs") &&
      normalizedCommandLine.includes("dev:desktop");
    const matchesDesktopPort =
      normalizedCommandLine.includes(`-p ${port}`) ||
      normalizedCommandLine.includes(`:${port}`);

    if ((isNextProcess || isDevProxyProcess) && (matchesDesktopPort || isDevProxyProcess || index > 0)) {
      return true;
    }

    currentPid = processInfo.ParentProcessId;
  }

  return false;
}

function terminateWindowsProcessTree(pid) {
  const result = spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], {
    encoding: "utf8",
  });

  if (result.error) {
    return false;
  }

  if (result.status === 0) {
    return true;
  }

  const combinedOutput = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
  return /not found|no running instance|does not exist/i.test(combinedOutput);
}

function createDesktopDevProxy() {
  const server = http.createServer((request, response) => {
    const proxyRequest = http.request(
      {
        hostname: desktopDevHost,
        port: desktopNextPort,
        path: request.url,
        method: request.method,
        headers: {
          ...request.headers,
          host: `${desktopDevHost}:${desktopNextPort}`,
        },
      },
      (proxyResponse) => {
        response.writeHead(proxyResponse.statusCode ?? 502, proxyResponse.headers);
        proxyResponse.pipe(response);
      },
    );

    proxyRequest.on("error", (error) => {
      response.writeHead(502, { "content-type": "text/plain; charset=utf-8" });
      response.end(`Next dev proxy error: ${error.message}`);
    });

    request.pipe(proxyRequest);
  });

  server.on("upgrade", (request, socket, head) => {
    const upstream = net.connect(desktopNextPort, desktopDevHost, () => {
      upstream.write(
        [
          `${request.method} ${request.url} HTTP/${request.httpVersion}`,
          `Host: ${desktopDevHost}:${desktopNextPort}`,
          ...Object.entries(request.headers)
            .filter(([key]) => key.toLowerCase() !== "host")
            .map(([key, value]) => `${key}: ${value}`),
          "",
          "",
        ].join("\r\n"),
      );
      if (head.length > 0) {
        upstream.write(head);
      }
      upstream.pipe(socket);
      socket.pipe(upstream);
    });

    upstream.on("error", () => {
      socket.destroy();
    });
  });

  server.on("error", (error) => {
    console.error(`前端开发代理启动失败: ${error.message}`);
    process.exit(1);
  });

  server.listen(desktopDevPort, desktopDevHost, () => {
    console.log(
      `前端开发代理已就绪: http://${desktopDevHost}:${desktopDevPort} -> http://${desktopDevHost}:${desktopNextPort}`,
    );
  });

  return server;
}

async function cleanupStaleDesktopDevState() {
  const listenerPids = [
    ...new Set([
      ...listDesktopDevListenerPids(desktopDevPort),
      ...listDesktopDevListenerPids(desktopNextPort),
    ]),
  ];

  for (const pid of listenerPids) {
    const processInfo = getWindowsProcessInfo(pid);
    if (!isDesktopDevProcess(pid, desktopDevPort) && !isDesktopDevProcess(pid, desktopNextPort)) {
      console.error(
        `开发端口被其他进程占用，无法自动清理。PID: ${pid}，命令行: ${processInfo?.CommandLine || "未知"}`,
      );
      process.exit(1);
    }

    console.log(`检测到未响应的 Next.js 开发进程，准备终止: PID ${pid}`);
    if (!terminateWindowsProcessTree(pid)) {
      console.error(`终止残留 Next.js 开发进程失败: PID ${pid}`);
      process.exit(1);
    }
  }

  if (listenerPids.length > 0) {
    const portReleased = await waitForDesktopDevPortsToClose();
    if (!portReleased) {
      console.error(`开发端口释放超时，无法继续启动前端开发服务`);
      process.exit(1);
    }
  }

  const desktopDevLockPath = getDesktopDevLockPath(frontendDir);
  if (existsSync(desktopDevLockPath)) {
    try {
      rmSync(desktopDevLockPath, { force: true });
      console.log(`已清理未响应的 Next.js 开发锁文件: ${desktopDevLockPath}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`清理 Next.js 开发锁文件失败: ${message}`);
      process.exit(1);
    }
  }
}

function resolvePnpmCommand() {
  const baseArgs =
    task === "dev:desktop"
      ? [
          "--dir",
          frontendDir,
          "exec",
          "next",
          "dev",
          "--webpack",
          "-H",
          desktopDevHost,
          "-p",
          String(desktopNextPort),
        ]
      : ["--dir", frontendDir, "run", task];
  const nodeBinDir = dirname(process.execPath);
  const windowsCandidates = [
    { command: resolve(nodeBinDir, "pnpm.cmd"), args: baseArgs },
    { command: resolve(nodeBinDir, "corepack.cmd"), args: ["pnpm", ...baseArgs] },
    { command: "pnpm.cmd", args: baseArgs },
    { command: "corepack.cmd", args: ["pnpm", ...baseArgs] },
  ];
  const defaultCandidates = [
    { command: "pnpm", args: baseArgs },
    { command: "corepack", args: ["pnpm", ...baseArgs] },
  ];

  const candidates = process.platform === "win32" ? windowsCandidates : defaultCandidates;
  const existingPathCandidates = candidates.filter(
    (candidate) => !candidate.command.includes(":") || existsSync(candidate.command),
  );

  for (const candidate of existingPathCandidates) {
    const probeArgs = candidate.args[0] === "pnpm" ? ["pnpm", "--version"] : ["--version"];
    const probe = spawnSync(candidate.command, probeArgs, {
      encoding: "utf8",
      shell: process.platform === "win32" && /\.cmd$/i.test(candidate.command),
      stdio: "ignore",
    });
    if (!probe.error && probe.status === 0) {
      return candidate;
    }
  }

  return candidates[candidates.length - 1];
}

const frontendDir = candidates.find(hasFrontendPackage);
if (!frontendDir) {
  console.error(`前端项目目录不存在，当前工作目录: ${cwd}`);
  process.exit(1);
}

if (task === "build:desktop" && hasBuiltFrontendDist(frontendDir)) {
  console.log(`前端产物已存在，跳过重复构建: ${resolve(frontendDir, "out", "index.html")}`);
  process.exit(0);
}

if (task === "dev:desktop") {
  if (await hasReusableDesktopDevServer()) {
    console.log(`检测到现有前端开发服务，直接复用: http://${desktopDevHost}:${desktopDevPort}`);
    process.exit(0);
  }

  const desktopDevLockPath = getDesktopDevLockPath(frontendDir);
  const hasDesktopDevLock = existsSync(desktopDevLockPath);
  const hasDesktopDevPortListener = await canConnect(desktopDevHost, desktopDevPort, 300);
  const hasDesktopNextPortListener = await canConnect(desktopDevHost, desktopNextPort, 300);
  if (hasDesktopDevLock || hasDesktopDevPortListener || hasDesktopNextPortListener) {
    const staleState = [
      hasDesktopDevLock ? "锁文件" : null,
      hasDesktopDevPortListener ? `${desktopDevPort}端口占用` : null,
      hasDesktopNextPortListener ? `${desktopNextPort}端口占用` : null,
    ]
      .filter(Boolean)
      .join(" / ");
    console.log(`检测到 Next.js 开发态残留（${staleState}），等待现有实例就绪: ${desktopDevLockPath}`);

    if (await waitForReusableDesktopDevServer()) {
      console.log(`检测到现有前端开发服务，直接复用: http://${desktopDevHost}:${desktopDevPort}`);
      process.exit(0);
    }

    await cleanupStaleDesktopDevState();
  }
}

const packageManager = resolvePnpmCommand();
console.log(`执行前端任务: ${packageManager.command} ${packageManager.args.join(" ")}`);
const needsShell = process.platform === "win32" && /\.cmd$/i.test(packageManager.command);

if (task === "dev:desktop") {
  const child = spawn(packageManager.command, packageManager.args, {
    stdio: "inherit",
    shell: needsShell,
    windowsHide: true,
  });
  child.once("error", (error) => {
    console.error(`前端开发服务启动失败: ${error.message}`);
    process.exit(1);
  });
  await warmupDesktopDevServer();
  createDesktopDevProxy();
  child.on("exit", (code, signal) => {
    if (signal) {
      process.exit(0);
    }
    process.exit(code ?? 0);
  });
  await new Promise(() => {});
}

const result = spawnSync(packageManager.command, packageManager.args, {
  stdio: "inherit",
  shell: needsShell,
});

if (result.error) {
  console.error(`前端构建启动失败: ${result.error.message}`);
  process.exit(1);
}

if (typeof result.status === "number" && result.status !== 0) {
  process.exit(result.status);
}

process.exit(0);

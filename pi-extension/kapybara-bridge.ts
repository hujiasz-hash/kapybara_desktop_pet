/**
 * Kapybara Bridge — 把 pi 的 session 生命周期推给桌面卡皮巴拉
 *
 * 做什么：pi 开始干活 / 干完 / 新 session / session 关闭时，POST 一个事件给
 *         桌面宠物（kapybara-buddy），让它演对应动画（打工 / 欢呼 / 捧爱心 / 睡觉）。
 *
 * 通道：http://127.0.0.1:17898/pi-event（桌宠起的本地回环 HTTP 服务，仅本机可访问）
 *
 * 关键设计：桌宠没开、端口不通、超时——一律静默忽略，绝不影响 pi 的任何行为。
 *          事件只带元信息（类型/cwd），不带对话内容。
 *
 * 事件选择说明：用 agent_settled 而不是 agent_end——agent_end 之后 pi 可能还会
 *              自动重试、压缩后重试或处理排队消息，只有 agent_settled 才代表
 *              "pi 不会再自动继续了"，用它才不会让卡皮巴拉在该干活时欢呼。
 */

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const KAPY_URL = process.env.KAPY_EVENT_URL || "http://127.0.0.1:17898/pi-event";
const TIMEOUT_MS = 250;

async function notifyKapy(payload: Record<string, unknown>): Promise<void> {
	try {
		await fetch(KAPY_URL, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(payload),
			signal: AbortSignal.timeout(TIMEOUT_MS),
		});
	} catch {
		// 桌宠没启动 / 端口被占 / 超时：静默。桥接是锦上添花，不能让 pi 出错。
	}
}

export default function (pi: ExtensionAPI) {
	let cwd: string | undefined;

	pi.on("session_start", async (_event, ctx) => {
		try {
			cwd = ctx.cwd;
		} catch {
			cwd = undefined;
		}
		await notifyKapy({ type: "session_start", cwd });
	});

	pi.on("agent_start", async () => {
		await notifyKapy({ type: "agent_start", cwd });
	});

	// agent_settled：pi 不会再自动继续（重试/压缩/排队都已结束）才有此事件
	pi.on("agent_settled", async () => {
		await notifyKapy({ type: "agent_settled", cwd });
	});

	// session_shutdown 可能来不及等 fetch 完成（进程就退了），但 250ms 超时足够本地回环
	pi.on("session_shutdown", async () => {
		await notifyKapy({ type: "session_shutdown", cwd });
	});
}

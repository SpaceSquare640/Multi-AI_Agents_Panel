/**
 * A brand-new user's first launch, in the order they would meet things.
 * Runs first (see wdio.conf.mjs): the onboarding step only appears while
 * there are no agents at all.
 */

import { FAKE_MODEL } from "../fake-ollama.mjs";
import { en, fake, fill, invoke, waitForApp } from "../lib/app.mjs";

describe("first launch", () => {
  before(async () => {
    await waitForApp();
    await fake("/__reset", {});
  });

  it("onboarding offers a first agent, and one click lands in a chat with it (#8)", async () => {
    const confirm = await $('.onboard-confirm input[type="checkbox"]');
    await confirm.waitForExist();
    await confirm.click();
    await $(`button=${en.onboarding.continue}`).click();

    await expect($(".first-agent-title")).toHaveText(en.firstAgent.title);

    const create = await $(`button=${fill(en.firstAgent.createWith, { provider: "Ollama", model: FAKE_MODEL })}`);
    await create.waitForExist({ timeout: 30_000 });
    await create.click();

    await $(".onboard").waitForExist({ reverse: true });
    await $('textarea[id^="composer-"]').waitForExist();

    const agents = await invoke("list_agents");
    expect(agents).toHaveLength(1);
    // An English interface gets the plain agent, not the Traditional
    // Chinese Daily Assistant template.
    expect(agents[0]).toMatchObject({
      name: en.firstAgent.defaultName,
      providerName: "ollama",
      model: FAKE_MODEL,
      systemPrompt: null,
    });
  });

  it("a message sent from the composer comes back with the model's reply", async () => {
    await fake("/__control", { delayMs: 0, reply: "hello from the fake model" });
    const composer = await $('textarea[id^="composer-"]');
    await composer.setValue("hi");
    await browser.keys("Enter");

    await expect($('.msg[data-from="agent"] .msg-body')).toHaveText("hello from the fake model", { wait: 20_000 });
  });

  it("the import-skill button shows its label, not a translation key (#3)", async () => {
    await expect($(`button=${en.chat.importCustomSkill}`)).toExist();
    await expect($("button=chat.importSkill")).not.toExist();
  });

  it("an existing agent's fallback chain can be opened, added to and emptied (#4)", async () => {
    await $(`button[title^="${en.shell.nav.models} —"]`).click();

    const [agent] = await invoke("list_agents");
    const toggle = await $(`button[aria-label="${fill(en.chat.agentFallbackChain, { name: agent.name })}"]`);
    await toggle.waitForExist();
    await toggle.click();

    const panel = await $(".agent-fallback");
    await expect(panel).toHaveText(en.chat.noneConfigured, { containing: true });

    await panel.$("select").selectByAttribute("value", "ollama");
    await panel.$("input").setValue("fallback-model");
    await panel.$(`button=${en.chat.addFallback}`).click();

    const step = "ollama/fallback-model";
    await expect(panel).toHaveText(`1. ${step}`, { containing: true });
    const chain = await invoke("list_agent_fallback_providers", { agentId: agent.id });
    expect(chain.map((s) => `${s.providerName}/${s.model}`)).toEqual([step]);

    await panel.$(`button[aria-label="${fill(en.chat.removeFallbackStep, { step })}"]`).click();
    await expect(panel).toHaveText(en.chat.noneConfigured, { containing: true });
    expect(await invoke("list_agent_fallback_providers", { agentId: agent.id })).toEqual([]);
  });

  it("each group speaker is told who is present and that it is their turn (#2)", async () => {
    const [first] = await invoke("list_agents");
    const second = await invoke("create_agent", {
      name: "Critic",
      roleTemplate: null,
      systemPrompt: null,
      providerKind: "local",
      providerName: "ollama",
      model: FAKE_MODEL,
    });
    const group = await invoke("create_group_session", { title: "E2E group", agentIds: [first.id, second.id] });

    await fake("/__reset", {});
    await fake("/__control", { delayMs: 0, reply: "my view" });

    const turn = await invoke("send_group_message", { sessionId: group.id, content: "Should we ship on Friday?" });
    expect(turn.kind).toBe("message");
    await invoke("advance_group_turn", { sessionId: group.id });

    const [firstRequest, secondRequest] = await fake("/__log");

    // What the first speaker saw.
    const firstNote = firstRequest.messages.at(-1);
    expect(firstNote.role).toBe("user");
    expect(firstNote.content.startsWith(`[Moderator: it is now ${first.name}'s turn.]`)).toBe(true);
    const context = firstRequest.messages.find(
      (m) => m.role === "system" && m.content.startsWith("You are taking part in a group discussion as"),
    );
    expect(context.content).toContain(`as ${first.name}.`);
    expect(context.content).toContain(`Other participants: ${second.name}.`);

    // What the second speaker saw: the first one's words, attributed,
    // and then its own turn note.
    expect(
      secondRequest.messages.some((m) => m.role === "user" && m.content === `[${first.name}]: my view`),
    ).toBe(true);
    expect(secondRequest.messages.at(-1).content.startsWith(`[Moderator: it is now ${second.name}'s turn.]`)).toBe(
      true,
    );
  });
});

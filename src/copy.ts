export type View = "gallery" | "create" | "library";

const messages = {
  zhCN: {
    views: {
      gallery: {
        kicker: "主题画廊",
        title: "浏览你的 WorkBuddy 外观",
        detail: "选择已生成的主题，直接应用到 WorkBuddy。",
      },
      create: {
        kicker: "从图片生成",
        title: "用图片生成你的 WorkBuddy 主题",
        detail: "本机取色、本机保存、自动应用。图片不会离开设备。",
      },
      library: {
        kicker: "我的主题",
        title: "管理保存在本机的主题",
        detail: "主题仅保存于本机，可随时应用或删除。",
      },
    },
    emptyGallery: {
      title: "暂时没有可用主题",
      detail: "重新启动应用后再试，或前往“从图片生成”创建自己的主题。",
    },
  },
} as const;

export type Locale = keyof typeof messages;

export function getMessages(locale: Locale = "zhCN") {
  return messages[locale];
}

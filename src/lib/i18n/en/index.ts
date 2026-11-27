// Catálogo inglês da interface (spec 20): português → inglês, um arquivo
// por área. A mesma frase em duas áreas precisa da mesma tradução
// (`tests/i18n.test.mjs`).

import home from "./home";
import mail from "./mail";
import settings from "./settings";
import shell from "./shell";
import tasks from "./tasks";

export const en: Record<string, string> = Object.assign({}, home, mail, settings, shell, tasks);

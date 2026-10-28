// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { mount } from "svelte";
import "dockview/dist/styles/dockview.css";
// UI font, bundled with the app (SIL Open Font License). Nothing loads from the network.
import "@fontsource/source-sans-3/latin-400.css";
import "@fontsource/source-sans-3/latin-600.css";
// Mall Pastel '95 font, bundled the same way.
import "@fontsource/nunito/latin-600.css";
import "@fontsource/nunito/latin-800.css";
import "./styles/tokens.css";
import "./styles/dock-theme.css";
import App from "./app/App.svelte";
import { installNativeMenuBlock } from "./app/native-menu";

installNativeMenuBlock();

const target = document.getElementById("app");
if (!target) {
  throw new Error("Missing #app mount point.");
}

export default mount(App, { target });

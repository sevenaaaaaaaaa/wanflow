import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";
import { installDevMock } from "./dev/mock";

installDevMock();
createApp(App).mount("#app");

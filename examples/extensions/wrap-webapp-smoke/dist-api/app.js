const response = await fetch("/api/smoke");
document.querySelector("#app").textContent = await response.text();

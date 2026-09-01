const originLabel = document.getElementById("origin");
if (originLabel) {
	originLabel.textContent = `App origin: ${window.location.origin}`;
}

const button = document.getElementById("ping");
const pong = document.getElementById("pong");
if (button && pong) {
	button.addEventListener("click", () => {
		pong.hidden = false;
	});
}

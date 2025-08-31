
const words = {
  extensions:
    'Extensions are software programs, built on web technologies (such as HTML, CSS, and JavaScript) that enable users to customize the Chrome browsing experience.',
  popup:
    "A UI surface which appears when an extension's action icon is clicked."
};

chrome.storage.session.get('lastWord', ({ lastWord }) => {
  updateDefinition(lastWord);
});

chrome.storage.session.onChanged.addListener((changes) => {
  const lastWordChange = changes['lastWord'];

  if (!lastWordChange) {
    return;
  }

  updateDefinition(lastWordChange.newValue);
});

function updateDefinition(word) {
  // If the side panel was opened manually, rather than using the context menu,
  // we might not have a word to show the definition for.
  if (!word) return;

  // Hide instructions.
  document.body.querySelector('#select-a-word').style.display = 'none';

  // Show word and definition.
  document.body.querySelector('#definition-word').innerText = word;
  document.body.querySelector('#definition-text').innerText =
    words[word.toLowerCase()] ??
    `Unknown word! Supported words: ${Object.keys(words).join(', ')}`;
}

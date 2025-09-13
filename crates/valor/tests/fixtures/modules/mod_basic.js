// Basic side-effect module for testing: write to #out
export const nothing = 0; // ensure this is a module
const el = document.getElementById('out');
if (el) { el.textContent = 'mod_ext'; }

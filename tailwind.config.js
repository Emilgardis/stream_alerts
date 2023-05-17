/** @type {import('tailwindcss').Config} */
module.exports = {
  content: {
    relative: true,
    files: ["*.html", "./src/**/*.rs"],
  },
  safelist: ["invisible"],
  theme: {
    extend: {},
  },
  plugins: [],
};

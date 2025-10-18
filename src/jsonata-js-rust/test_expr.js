const jsonata = require("../jsonata/jsonata");
(async () => {
  try {
    const expr = String.raw`$match("test escape \\" \\t", /\\/);`;
    const result = await jsonata(expr).evaluate({});
    console.log("RESULT", result);
  } catch (err) {
    console.error("ERR", err);
    process.exit(1);
  }
})();

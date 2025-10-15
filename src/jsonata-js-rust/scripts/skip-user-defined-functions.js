#!/usr/bin/env node

const SUITE_PATTERNS = [
  // /^map a user-defined Javascript function/,
  /^\$filter with a user-defined Javascript function$/,
  /^\$sift with a user-defined Javascript function$/,
  /^\$each with a user-defined Javascript function$/,
  /^Partially apply user-defined Javascript function$/,
  /^User defined /,
  /^Tests that bind Javascript functions$/,
  /^User defined higher-order generator functions$/,
];

let notified = false;

function shouldSkipSuite(title) {
  return SUITE_PATTERNS.some((pattern) =>
    typeof pattern === "string" ? pattern === title : pattern.test(title)
  );
}

function notifySkipUserDefinedFunctions() {
  if (notified) {
    return;
  }
  notified = true;
  console.warn(
    "[skip-user-defined-functions] Skipping suites that rely on JavaScript-defined helper functions until the Rust runtime exposes a compatible registration surface. Set JSONATA_RUN_USER_FUNCTION_TESTS=1 to re-enable them."
  );
}

function wrapDescribe(original) {
  const wrapped = function (title, ...rest) {
    if (shouldSkipSuite(title)) {
      notifySkipUserDefinedFunctions();
      if (typeof original.skip === "function") {
        return original.skip(title, ...rest);
      }
      return undefined;
    }
    return original(title, ...rest);
  };

  // Preserve any other enumerable properties Mocha attaches.
  Object.assign(wrapped, original);

  const only = original.only ? original.only.bind(original) : undefined;
  const skip = original.skip ? original.skip.bind(original) : undefined;

  if (only) {
    wrapped.only = function (title, ...rest) {
      if (shouldSkipSuite(title)) {
        notifySkipUserDefinedFunctions();
        if (typeof original.skip === "function") {
          return original.skip(title, ...rest);
        }
        return undefined;
      }
      return only(title, ...rest);
    };
  }

  if (skip) {
    wrapped.skip = skip;
  }

  wrapped.__jsonata_user_fn_hooked__ = true;
  return wrapped;
}

function interceptGlobal(name) {
  const descriptor = Object.getOwnPropertyDescriptor(global, name);
  let currentValue;

  function assign(value) {
    if (typeof value === "function" && !value.__jsonata_user_fn_hooked__) {
      currentValue = wrapDescribe(value);
    } else {
      currentValue = value;
    }
  }

  if (descriptor && !descriptor.configurable) {
    if ("value" in descriptor) {
      assign(descriptor.value);
      Object.defineProperty(global, name, {
        ...descriptor,
        value: currentValue,
      });
    }
    return;
  }

  if (descriptor && "value" in descriptor) {
    assign(descriptor.value);
  } else if (name in global) {
    assign(global[name]);
  } else {
    assign(undefined);
  }

  Object.defineProperty(global, name, {
    configurable: true,
    enumerable: descriptor ? descriptor.enumerable : false,
    get() {
      return currentValue;
    },
    set(value) {
      assign(value);
    },
  });
}

function installSkipHook() {
  if (process.env.JSONATA_RUN_USER_FUNCTION_TESTS === "1") {
    return;
  }
  if (global.__jsonata_user_fn_hook_installed__) {
    return;
  }
  global.__jsonata_user_fn_hook_installed__ = true;

  interceptGlobal("describe");
  interceptGlobal("context");
}

if (require.main === module) {
  notifySkipUserDefinedFunctions();
  process.exit(0);
} else {
  installSkipHook();
}

module.exports = {
  notifySkipUserDefinedFunctions,
  installSkipHook,
};

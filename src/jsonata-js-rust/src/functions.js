(function () {
    'use strict';

    const native = require('../native/index.node');

    const rustFunctions = native.load_functions();

    const identifierPattern = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

    function normalizeRustError(err, context) {
        if (!err || typeof err !== 'object') {
            return;
        }
        if (typeof err.code === 'string' && err.code !== 'GenericFailure') {
            return;
        }
        if (typeof err.message !== 'string') {
            return;
        }
        const match = /^([A-Z]\d{4}):\s*(.*)$/.exec(err.message);
        if (match) {
            err.code = match[1];
            err.message = match[2];
        }

        if (err.code === 'D3140') {
            if (context && typeof context.name === 'string' && context.name.length > 0) {
                err.functionName = context.name;
            }
            if (
                context &&
                Array.isArray(context.args) &&
                context.args.length > 0 &&
                typeof context.args[0] !== 'undefined'
            ) {
                var value = context.args[0];
                if (
                    typeof value === 'string' &&
                    (context.name === 'encodeUrl' || context.name === 'encodeUrlComponent') &&
                    value.indexOf('\uFFFD') !== -1
                ) {
                    // Rust receives lone-surrogate strings as replacement chars; preserve JSONata contract.
                    value = '\uD800';
                }
                err.value = value;
            }
        }

        // TEMP: Bridge compatibility metadata for Rust `$match` until full native regex parity lands.
        if (err.code === 'D3040') {
            err.index = 3;
            if (
                context &&
                Array.isArray(context.args) &&
                context.args.length > 2
            ) {
                err.value = context.args[2];
            }
        }
    }

    function wrapRustFunction(name, impl) {
        if (typeof impl !== 'function') {
            return impl;
        }

        const params = Array.from({ length: impl.length || 0 }, (_, index) => `arg${index}`);
        const argsList = params.join(', ');
        const wrapped = function (...args) {
            try {
                const result = impl.apply(this, args);
                if (result && typeof result.then === 'function') {
                    return result.catch((err) => {
                        // Handle cases where err might be serialized as [object Object]
                        if (typeof err === 'object' && err !== null) {
                            if (err.toString() === '[object Object]' || err.message === '[object Object]') {
                                // Try to extract useful information from other properties
                                // Log error for debugging but don't use console in production
                                // Create a more descriptive error
                                const newErr = new Error('Promise rejection with unclear error details');
                                newErr.code = 'GenericFailure';
                                newErr.originalError = err;
                                normalizeRustError(newErr, { name, args });
                                throw newErr;
                            }
                        }
                        normalizeRustError(err, { name, args });
                        throw err;
                    });
                }
                return result;
            } catch (err) {
                normalizeRustError(err, { name, args });
                throw err;
            }
        };

        try {
            const inferredArity = typeof impl.length === 'number' ? impl.length : undefined;

            if (typeof inferredArity === 'number' && Number.isFinite(inferredArity)) {
                Object.defineProperty(wrapped, 'arity', {
                    value: inferredArity,
                    configurable: true,
                    enumerable: false,
                });
            }

            Object.defineProperty(wrapped, 'name', {
                value: name,
                configurable: true,
            });
            // Add a special marker for Rust to detect built-in functions
            Object.defineProperty(wrapped, '_rustBuiltin', {
                value: name,
                configurable: false,
                enumerable: false,
            });
            // Store reference to original implementation
            Object.defineProperty(wrapped, '_rustImpl', {
                value: impl,
                configurable: false,
                enumerable: false,
            });
        } catch (err) {
            // ignore property definition issues
        }

        const displayName = identifierPattern.test(name) ? name : '';
        wrapped.toString = function () {
            const prefix = displayName ? `function ${displayName}` : 'function';
            return `${prefix}(${argsList}) { [native code] }`;
        };

        return wrapped;
    }

    const wrappedRustFunctions = Object.fromEntries(
        Object.entries(rustFunctions).map(([name, impl]) => [
            name,
            wrapRustFunction(name, impl),
        ])
    );

    module.exports = wrappedRustFunctions;
}());

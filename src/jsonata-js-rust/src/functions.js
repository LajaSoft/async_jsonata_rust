(function () {
    'use strict';

    const native = require('../native/index.node');
    const upstreamFunctions = require('../../jsonata/src/functions');

    const rustFunctions = native.load_functions();

    const identifierPattern = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

    function extractParameters(fn) {
        if (typeof fn !== 'function') {
            return [];
        }
        const match = /\(([^)]*)\)/.exec(fn.toString());
        if (!match) {
            return [];
        }
        return match[1]
            .split(',')
            .map((arg) => arg.trim())
            .filter((arg) => arg.length > 0);
    }

    function normalizeRustError(err) {
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
    }

    function wrapRustFunction(name, impl) {
        if (typeof impl !== 'function') {
            return impl;
        }

        const upstream = upstreamFunctions[name];
        const params = extractParameters(upstream);
        const argsList = params.join(', ');
        const wrapped = function (...args) {
            if (name === 'string' && typeof upstream === 'function') {
                return upstream.apply(this, args);
            }
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
                                normalizeRustError(newErr);
                                throw newErr;
                            }
                        }
                        normalizeRustError(err);
                        throw err;
                    });
                }
                return result;
            } catch (err) {
                normalizeRustError(err);
                throw err;
            }
        };

        try {
            var inferredArity;
            if (typeof upstream === 'function') {
                inferredArity = upstream.length;
            } else if (typeof impl.length === 'number') {
                inferredArity = impl.length;
            }

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

    const mergedFunctions = {
        ...upstreamFunctions,
        ...wrappedRustFunctions,
    };

    const rustSort = wrappedRustFunctions.sort;
    const jsSort = upstreamFunctions.sort;

    if (typeof rustSort === 'function' && typeof jsSort === 'function') {
        mergedFunctions.sort = function (...args) {
            const comparator = args[1];
            if (typeof comparator === 'function') {
                return Promise.resolve(jsSort.apply(this, args));
            }
            return Promise.resolve(rustSort.apply(this, args));
        };
    }

    module.exports = mergedFunctions;
}());

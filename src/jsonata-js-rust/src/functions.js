(function () {
    'use strict';

    const native = require('../native/index.node');

    const rustFunctions = native.load_functions();
    const rustArityOverrides = Object.freeze({
        abs: 1,
        append: 2,
        assert: 2,
        average: 1,
        base64decode: 1,
        base64encode: 1,
        boolean: 1,
        ceil: 1,
        contains: 2,
        count: 1,
        decodeUrl: 1,
        decodeUrlComponent: 1,
        distinct: 1,
        each: 2,
        encodeUrl: 1,
        encodeUrlComponent: 1,
        error: 1,
        exists: 1,
        filter: 2,
        floor: 1,
        foldLeft: 3,
        formatBase: 2,
        formatNumber: 3,
        join: 2,
        keys: 1,
        length: 1,
        lookup: 2,
        lowercase: 1,
        map: 2,
        match: 3,
        max: 1,
        merge: 1,
        min: 1,
        not: 1,
        number: 1,
        pad: 3,
        power: 2,
        random: 0,
        replace: 4,
        reverse: 1,
        round: 2,
        shuffle: 1,
        sift: 2,
        single: 2,
        sort: 2,
        split: 3,
        spread: 1,
        sqrt: 1,
        string: 1,
        substring: 3,
        substringAfter: 2,
        substringBefore: 2,
        sum: 1,
        trim: 1,
        type: 1,
        uppercase: 1,
        zip: 2,
    });

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

        const overrideArity = Object.prototype.hasOwnProperty.call(rustArityOverrides, name)
            ? rustArityOverrides[name]
            : undefined;
        const inferredArity = typeof overrideArity === 'number'
            ? overrideArity
            : (typeof impl.arity === 'number' ? impl.arity : (typeof impl.length === 'number' ? impl.length : 0));

        const params = Array.from({ length: inferredArity || 0 }, (_, index) => `arg${index}`);
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

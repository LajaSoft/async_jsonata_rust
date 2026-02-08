/**
 * Native parser bridge backed by the Rust implementation. shall never contain fallbacks to JS parser.
 */

(function () {
    'use strict';

    const native = require('../native/index.node');

    function reviveRegexLiterals(node) {
        if (!node || typeof node !== 'object') {
            return node;
        }

        if (Array.isArray(node)) {
            node.forEach(reviveRegexLiterals);
            return node;
        }

        if (node.type === 'regex' && node.value && typeof node.value.pattern === 'string') {
            var flags = node.value.flags || 'g';
            try {
                node.value = new RegExp(node.value.pattern, flags);
            } catch (err) {
                throw ensureStack(err);
            }
        }

        Object.keys(node).forEach(function (key) {
            if (key === 'value' && node.type === 'regex') {
                return;
            }
            reviveRegexLiterals(node[key]);
        });

        return node;
    }

    function stripInternalFields(node) {
        if (!node || typeof node !== 'object') {
            return node;
        }

        if (Array.isArray(node)) {
            node.forEach(stripInternalFields);
            return node;
        }

        if (Object.prototype.hasOwnProperty.call(node, 'id')) {
            delete node.id;
        }

        Object.keys(node).forEach(function (key) {
            stripInternalFields(node[key]);
        });

        return node;
    }

    function callRustParser(source, recover) {
        if (!native || typeof native.parseExpression !== 'function') {
            throw ensureStack({
                code: 'S0001',
                message: 'Rust parser is unavailable'
            });
        }

        const result = native.parseExpression(source, recover);
        if (result && typeof result === 'object') {
            if (result.ok === true && result.ast) {
                stripInternalFields(result.ast);
                reviveRegexLiterals(result.ast);
            }
            return result;
        }

        throw ensureStack({
            code: 'S0002',
            message: 'Rust parser returned invalid response'
        });
    }

    function ensureStack(error) {
        if (error && typeof error === 'object' && !error.stack) {
            error.stack = (new Error()).stack;
        }
        return error;
    }

    function parser(source, recover) {
        const rustResult = callRustParser(source, Boolean(recover));
        if (rustResult.ok === true) {
            return rustResult.ast;
        }
        if (rustResult.error) {
            throw ensureStack(rustResult.error);
        }
        throw ensureStack({
            code: 'S0003',
            message: 'Rust parser did not return an AST'
        });
    }

    module.exports = parser;
}());

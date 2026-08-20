const WebSocket = require('ws');
const apiKey = process.env.VITE_GEMINI_API_KEY;

function testModel(modelName) {
    const url = `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent?key=${apiKey}`;
    console.log("URL:", url);
    const ws = new WebSocket(url);

    ws.on('open', () => {
        console.log(`Connected for ${modelName}`);
        const setupMsg = {
            setup: {
                model: `models/${modelName}`,
                generation_config: {
                    response_modalities: ["AUDIO"],
                    speech_config: {
                        voice_config: {
                            prebuilt_voice_config: {
                                voice_name: "Aoede"
                            }
                        },
                        language_code: "es"
                    },
                    translation_config: {
                        target_language_code: "es",
                        echo_target_language: true
                    }
                }
            }
        };
        ws.send(JSON.stringify(setupMsg));
    });

    ws.on('message', (data) => {
        console.log(`Received for ${modelName}:`, data.toString());
        ws.close();
    });

    ws.on('close', (code, reason) => {
        console.log(`Closed for ${modelName} with code ${code} reason: ${reason.toString()}`);
    });
}

testModel("gemini-3.5-live-translate-preview");

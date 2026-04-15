# Attention Mechanisms

Attention allows models to focus on relevant parts of the input. The scaled dot-product attention computes softmax(QK^T / sqrt(d_k)) * V.

This mechanism is central to [[ai/transformers]] and has applications beyond NLP in vision, audio, and multimodal models.

The Co-Wiki's [[ai/spreading-activation]] is conceptually related to attention: both involve weighted propagation through a network of associations.

// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The settings window in Qt Widgets. Rows come from settings.shcl through the
// Rust side, and every change goes back there, so this file only lays out
// controls and copies state onto them. Same model as SilkTerm's dialog: tabs
// down the left, a revert arrow per row, and Cancel, Apply, OK.

#include "tukzmon-settings/src/qt.h"
#include "tukzmon-settings/src/bridge.rs.h"

#include <QAbstractItemView>
#include <QAccessible>
#include <QApplication>
#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QCursor>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QElapsedTimer>
#include <QFormLayout>
#include <QFrame>
#include <QKeyEvent>
#include <QGridLayout>
#include <QImage>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QLabel>
#include <QLineEdit>
#include <QLinearGradient>
#include <QListWidget>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QProxyStyle>
#include <QPushButton>
#include <QRadioButton>
#include <QScreen>
#include <QRegularExpressionValidator>
#include <QScrollArea>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QSlider>
#include <QSocketNotifier>
#include <QSpinBox>
#include <QStackedWidget>
#include <QStyle>
#include <QStyleOptionButton>
#include <QStyleOptionSlider>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QVBoxLayout>
#include <QWindow>

#include <dlfcn.h>
#include <fcntl.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <csignal>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <functional>
#include <initializer_list>
#include <map>
#include <string>
#include <string_view>
#include <tuple>
#include <utility>
#include <memory>
#include <vector>

namespace tukzmon {
namespace {

QString qs(const rust::String &s) { return QString::fromUtf8(s.data(), int(s.size())); }

// The style's own selection is a solid block meant for highlighted text, and in
// a dark theme that's white on bright blue. A tint of it under the plain text
// color reads in a light theme and a dark one.
void softenSelection(QWidget *w) {
	QPalette pal = w->palette();
	for (auto group : {QPalette::Active, QPalette::Inactive}) {
		const QColor base = pal.color(group, QPalette::Base);
		const QColor pick = pal.color(group, QPalette::Highlight);
		auto mix = [](float a, float b) { return a + ((b - a) * 0.35F); };
		const QColor tint = QColor::fromRgbF(mix(base.redF(), pick.redF()), mix(base.greenF(), pick.greenF()),
			mix(base.blueF(), pick.blueF()));
		pal.setColor(group, QPalette::Highlight, tint);
		pal.setColor(group, QPalette::HighlightedText, pal.color(group, QPalette::Text));
	}
	w->setPalette(pal);
}

// The parts set in, the pages that scroll and the tab list, sit darker than the
// rest of the window, so a page reads apart from what stays put. The two shades
// span the step from the theme's window color 1/16 of the way to its text
// color, with the inset parts at the darker end and the rest 3/4 of the way to
// the lighter. So a dark theme keeps its own window color for the inset parts.
QColor shadeOf(const QColor &back, const QColor &ink, bool inset) {
	const float step = 1.0F / 16;
	const float darker = ink.lightnessF() > back.lightnessF() ? 0.0F : step;
	const float lighter = step - darker;
	const float f = inset ? darker : darker + ((lighter - darker) * 3 / 4);
	auto mix = [f](float a, float b) { return a + ((b - a) * f); };
	return QColor::fromRgbF(mix(back.redF(), ink.redF()), mix(back.greenF(), ink.greenF()),
		mix(back.blueF(), ink.blueF()));
}

// From the app's palette, so a widget shaded before it has a parent gets the
// same as one shaded after.
void shade(QWidget *w, bool inset) {
	const QPalette theme = QApplication::palette();
	QPalette pal = w->palette();
	for (auto group : {QPalette::Active, QPalette::Inactive, QPalette::Disabled}) {
		const QColor c = shadeOf(theme.color(group, QPalette::Window), theme.color(group, QPalette::WindowText), inset);
		pal.setColor(group, QPalette::Window, c);
		// a list draws on Base
		if (inset && qobject_cast<QAbstractItemView *>(w))
			pal.setColor(group, QPalette::Base, c);
	}
	w->setPalette(pal);
}

QString wrapped(const QString &tip) {
	const QByteArray u = tip.toUtf8();
	return qs(wrap_tip(rust::Str(u.constData(), size_t(u.size()))));
}

// Qt shows a plain text tip as one line however long, so every tip in the
// window goes through here first.
class TipWrap final : public QObject {
protected:
	bool eventFilter(QObject *o, QEvent *e) override {
		if (e->type() != QEvent::ToolTip || !o->isWidgetType())
			return false;
		auto *w = static_cast<QWidget *>(o);
		// none of its own, so the widget passes it on to its parent
		if (w->toolTip().isEmpty())
			return false;
		QToolTip::showText(static_cast<QHelpEvent *>(e)->globalPos(), wrapped(w->toolTip()), w, QRect(),
			w->toolTipDuration());
		return true;
	}
};

// Room from a button's text to the inner edge of its frame, at the least.
// Fusion leaves 3 px over and under, and as little across on a long label.
// One floor for every button, so a row of them is one height.
constexpr int BUTTON_PAD_ACROSS = 12;
constexpr int BUTTON_PAD_DOWN = 8;

// Some dark themes draw an empty checkbox with an edge close to the window
// color, so it all but vanishes. An edge in the text color, a little faded,
// shows in a light theme and a dark one. Buttons also get more room around
// their text than most styles give, and a slider's handle is as tall as the
// slider.
class BoxStyle final : public QProxyStyle {
public:
	QRect subControlRect(ComplexControl cc, const QStyleOptionComplex *opt, SubControl sc,
		const QWidget *w) const override {
		QRect r = QProxyStyle::subControlRect(cc, opt, sc, w);
		const auto *s = qstyleoption_cast<const QStyleOptionSlider *>(opt);
		if (cc != CC_Slider || sc != SC_SliderHandle || !s || s->orientation != Qt::Horizontal ||
			s->tickPosition != QSlider::NoTicks)
			return r;
		r.setTop(s->rect.top());
		r.setBottom(s->rect.bottom());
		return r;
	}
	QSize sizeFromContents(ContentsType ct, const QStyleOption *opt, const QSize &csz,
		const QWidget *w) const override {
		QSize s = QProxyStyle::sizeFromContents(ct, opt, csz, w);
		const auto *b = qstyleoption_cast<const QStyleOptionButton *>(opt);
		if (ct != CT_PushButton || !b || b->text.isEmpty())
			return s;
		// the frame's inset, as SE_PushButtonContents takes it
		int inset = pixelMetric(PM_DefaultFrameWidth, opt, w);
		if (b->features & QStyleOptionButton::AutoDefaultButton)
			inset += pixelMetric(PM_ButtonDefaultIndicator, opt, w);
		return s.expandedTo(csz + QSize(2 * (inset + BUTTON_PAD_ACROSS), 2 * (inset + BUTTON_PAD_DOWN)));
	}
	void drawPrimitive(PrimitiveElement pe, const QStyleOption *opt, QPainter *p, const QWidget *w) const override {
		QProxyStyle::drawPrimitive(pe, opt, p, w);
		if (pe != PE_IndicatorCheckBox && pe != PE_IndicatorRadioButton)
			return;
		QColor edge = opt->palette.color(QPalette::WindowText);
		edge.setAlphaF(0.6F);
		p->save();
		p->setRenderHint(QPainter::Antialiasing);
		p->setPen(QPen(edge, 1));
		p->setBrush(Qt::NoBrush);
		const QRectF r = QRectF(opt->rect).adjusted(0.5, 0.5, -0.5, -0.5);
		if (pe == PE_IndicatorRadioButton)
			p->drawEllipse(r);
		else
			p->drawRoundedRect(r, 2, 2);
		p->restore();
	}
};

// A press on the track puts the handle under the pointer and goes on as a drag,
// where Qt's own slider pages toward it. A press on the handle drags it from
// where it was taken, so a grab never nudges it onto the next detent.
class TrackSlider final : public QSlider {
public:
	TrackSlider() : QSlider(Qt::Horizontal) {}
	QRect part(QStyle::SubControl sc) const {
		QStyleOptionSlider opt;
		initStyleOption(&opt);
		return style()->subControlRect(QStyle::CC_Slider, &opt, sc, this);
	}

protected:
	void mousePressEvent(QMouseEvent *e) override {
		QRect handle = part(QStyle::SC_SliderHandle);
		if (e->buttons() != Qt::LeftButton || handle.contains(e->position().toPoint())) {
			QSlider::mousePressEvent(e);
			return;
		}
		QStyleOptionSlider opt;
		initStyleOption(&opt);
		const QRect groove = part(QStyle::SC_SliderGroove);
		const int x = e->position().toPoint().x() - groove.x() - (handle.width() / 2);
		setSliderPosition(
			QStyle::sliderValueFromPosition(minimum(), maximum(), x, groove.width() - handle.width(), opt.upsideDown));
		// a detent can pull it off the pointer, so take hold of it where it went
		handle = part(QStyle::SC_SliderHandle);
		const QPointF at = handle.center();
		QMouseEvent grab(e->type(), at, mapToGlobal(at), e->button(), e->buttons(), e->modifiers(),
			e->pointingDevice());
		QSlider::mousePressEvent(&grab);
		e->setAccepted(grab.isAccepted());
	}
};

constexpr int SLIDER_STEPS = 1000;
constexpr int HEAD_GAP = 22;

// QScrollArea's own hint stops at about 24 lines, so a tall tab opened with a
// scroll bar it had no need for. Every graph's colors on one page is far
// taller than the rest, so that page asks for no height and scrolls in what
// the other tabs set.
class FitScroll final : public QScrollArea {
public:
	bool ownHeight = true;
	QSize sizeHint() const override {
		QSize s = QScrollArea::sizeHint();
		// its width stops short too, which squeezed the graph list's dropdowns
		if (widget())
			s.setWidth(std::max(s.width(),
				widget()->sizeHint().width() + (2 * frameWidth()) +
					(ownHeight ? 0 : verticalScrollBar()->sizeHint().width())));
		if (!ownHeight)
			s.setHeight(minimumSizeHint().height());
		else if (widget())
			s.setHeight(std::max(s.height(), widget()->sizeHint().height() + (2 * frameWidth())));
		return s;
	}
};

// The room is the screen less its panels, and the frame is the window
// manager's, which isn't known until the window is up.
QSize fitSize(QSize want, const QRect &room, const QMargins &frame) {
	return want.boundedTo(room.size().shrunkBy(frame));
}

// A press or drag on the square or the strip, or an arrow key on it.
class PickArea : public QWidget {
public:
	std::function<void(double, double)> moved;
	double h = 0, s = 0, v = 0;

protected:
	// x and y are 0 to 1 across the area, clamped, so a drag off the edge holds it there
	virtual void pickAt(double x, double y) = 0;
	virtual void arrow(int dx, int dy, double by) = 0;

	void mousePressEvent(QMouseEvent *e) override {
		if (e->button() == Qt::LeftButton)
			pickFrom(e);
	}
	void mouseMoveEvent(QMouseEvent *e) override {
		if (e->buttons() & Qt::LeftButton)
			pickFrom(e);
	}
	void keyPressEvent(QKeyEvent *e) override {
		const double by = e->modifiers() & Qt::ShiftModifier ? 0.1 : 0.01;
		switch (e->key()) {
		case Qt::Key_Left: arrow(-1, 0, by); break;
		case Qt::Key_Right: arrow(1, 0, by); break;
		case Qt::Key_Up: arrow(0, -1, by); break;
		case Qt::Key_Down: arrow(0, 1, by); break;
		default: QWidget::keyPressEvent(e);
		}
	}
	void edge(QPainter &p) {
		const bool on = hasFocus();
		p.setPen(QPen(palette().color(on ? QPalette::Highlight : QPalette::Mid), on ? 2 : 1));
		p.setBrush(Qt::NoBrush);
		p.drawRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5));
	}

private:
	void pickFrom(QMouseEvent *e) {
		const QPointF at = e->position();
		pickAt(std::clamp(at.x() / std::max(1, width() - 1), 0.0, 1.0),
			std::clamp(at.y() / std::max(1, height() - 1), 0.0, 1.0));
	}
};

// saturation left to right, brightness bottom to top, at one hue
class SvSquare final : public PickArea {
public:
	explicit SvSquare(int side) {
		setFixedSize(side, side);
		setFocusPolicy(Qt::StrongFocus);
		setCursor(Qt::CrossCursor);
	}

protected:
	void pickAt(double x, double y) override { moved(x, 1 - y); }
	void arrow(int dx, int dy, double by) override {
		moved(std::clamp(s + (dx * by), 0.0, 1.0), std::clamp(v - (dy * by), 0.0, 1.0));
	}
	void paintEvent(QPaintEvent *) override {
		QPainter p(this);
		p.fillRect(rect(), QColor::fromHsvF(float(h), 1, 1));
		QLinearGradient white(0, 0, width(), 0);
		white.setColorAt(0, QColor(255, 255, 255));
		white.setColorAt(1, QColor(255, 255, 255, 0));
		p.fillRect(rect(), white);
		QLinearGradient black(0, 0, 0, height());
		black.setColorAt(0, QColor(0, 0, 0, 0));
		black.setColorAt(1, QColor(0, 0, 0));
		p.fillRect(rect(), black);
		edge(p);
		const QColor now = QColor::fromHsvF(float(h), float(s), float(v));
		const bool light = 0.2126 * now.redF() + 0.7152 * now.greenF() + 0.0722 * now.blueF() > 0.5;
		p.setRenderHint(QPainter::Antialiasing);
		p.setPen(QPen(light ? Qt::black : Qt::white, 2));
		p.drawEllipse(QPointF(s * (width() - 1), (1 - v) * (height() - 1)), 5, 5);
	}
};

// the hue, top to bottom
class HueStrip final : public PickArea {
public:
	HueStrip(int w, int h) {
		setFixedSize(w, h);
		setFocusPolicy(Qt::StrongFocus);
	}

protected:
	void pickAt(double, double y) override { moved(y, 0); }
	void arrow(int dx, int dy, double by) override {
		if (dy)
			moved(std::fmod(h + (dy * by) + 1, 1), 0);
		else
			focusNextPrevChild(dx > 0);
	}
	void paintEvent(QPaintEvent *) override {
		QPainter p(this);
		QLinearGradient ramp(0, 0, 0, height());
		for (int k = 0; k <= 6; k++)
			ramp.setColorAt(k / 6.0, QColor::fromHsvF(k == 6 ? 0 : float(k) / 6.0F, 1, 1));
		p.fillRect(rect(), ramp);
		edge(p);
		// the strip is every hue, so its marker is black and white
		const double y = h * (height() - 1);
		p.fillRect(QRectF(0, y - 2.5, width(), 5), Qt::black);
		p.fillRect(QRectF(1, y - 1.5, width() - 2, 3), Qt::white);
	}
};

// For a dialog's destructor. Its members go before Qt deletes the children,
// whose handlers reach them, and a shown child that loses the focus or the
// pointer then fires one. So nothing of theirs runs once the members start
// going.
void quiet(QDialog *w) {
	for (QObject *o : w->findChildren<QObject *>())
		QObject::disconnect(o, nullptr, w, nullptr);
	w->hide();
}

// SilkTerm's picker, not the toolkit's. The row behind follows every change,
// and Cancel puts back all of it.
class ColorPicker final : public QDialog {
public:
	ColorPicker(Dialog &d, const QString &title, std::function<void()> changed, QWidget *parent)
		: QDialog(parent), d(d), changed(std::move(changed)) {
		setWindowTitle(title);
		const int line = fontMetrics().height();
		square = new SvSquare(line * 12);
		strip = new HueStrip(line * 3 / 2, line * 12);
		square->moved = [this](double s, double v) { set(state.h, s, v); };
		strip->moved = [this](double h, double) { set(h, state.s, state.v); };

		state = pick_state(d);
		auto *form = new QFormLayout;
		for (size_t i = 0; i < state.labels.size(); i++) {
			const bool hex = i >= state.percents.size();
			QWidget *box = nullptr;
			if (hex) {
				auto *edit = new QLineEdit;
				edit->setValidator(new QRegularExpressionValidator(QRegularExpression("#?[0-9a-fA-F]{0,6}"), edit));
				edit->setMaximumWidth(edit->fontMetrics().horizontalAdvance("#DDDDDDD") + 16);
				connect(edit, &QLineEdit::textEdited, this, [this, i, edit](const QString &t) {
					if (pick_type(this->d, i, t.toStdString()))
						sync(edit);
				});
				// a half-typed hex shows the color again on the way out
				connect(edit, &QLineEdit::editingFinished, this, [this] { sync(nullptr); });
				hexBox = edit;
				box = edit;
			} else {
				auto *spin = new QSpinBox;
				spin->setRange(0, 100);
				connect(spin, &QSpinBox::valueChanged, this, [this, i, spin](int v) {
					if (pick_type(this->d, i, std::to_string(v)))
						sync(spin);
				});
				spins.push_back(spin);
				box = spin;
			}
			form->addRow(qs(state.labels[i]), box);
		}

		auto *cancel = new QPushButton("Cancel");
		auto *ok = new QPushButton("OK");
		// Cancel stays auto default, so Enter on it cancels
		ok->setDefault(true);
		connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
		connect(ok, &QPushButton::clicked, this, &QDialog::accept);
		auto *buttons = new QHBoxLayout;
		buttons->addStretch(1);
		buttons->addWidget(cancel);
		buttons->addWidget(ok);

		auto *top = new QHBoxLayout;
		top->addWidget(square, 0, Qt::AlignTop);
		top->addWidget(strip, 0, Qt::AlignTop);
		top->addSpacing(line);
		top->addLayout(form);
		auto *outer = new QVBoxLayout(this);
		outer->addLayout(top);
		outer->addLayout(buttons);
		sync(nullptr);
		square->setFocus();
	}

	ColorPicker(const ColorPicker &) = delete;
	ColorPicker &operator=(const ColorPicker &) = delete;
	ColorPicker(ColorPicker &&) = delete;
	ColorPicker &operator=(ColorPicker &&) = delete;
	~ColorPicker() override { quiet(this); }

	void done(int r) override {
		pick_close(d, r == QDialog::Accepted);
		changed();
		QDialog::done(r);
	}

private:
	Dialog &d;
	std::function<void()> changed;
	PickState state;
	SvSquare *square = nullptr;
	HueStrip *strip = nullptr;
	std::vector<QSpinBox *> spins;
	QLineEdit *hexBox = nullptr;

	void set(double h, double s, double v) {
		pick_set(d, h, s, v);
		sync(nullptr);
	}

	// `typing` is the box being typed in, which keeps its own text
	void sync(QWidget *typing) {
		state = pick_state(d);
		for (PickArea *a : {static_cast<PickArea *>(square), static_cast<PickArea *>(strip)}) {
			a->h = state.h;
			a->s = state.s;
			a->v = state.v;
			a->update();
		}
		for (size_t i = 0; i < spins.size(); i++) {
			if (spins[i] == typing)
				continue;
			const QSignalBlocker b(spins[i]);
			spins[i]->setValue(state.percents[i]);
		}
		if (hexBox != typing) {
			const QSignalBlocker b(hexBox);
			hexBox->setText(qs(state.hex));
		}
		changed();
	}
};

// The handle at the right end of a graph list row. Dragging it moves the row,
// and so do Up and Down while it has focus.
class Gripper final : public QWidget {
public:
	std::function<void(QPoint)> dragged;
	std::function<void(int)> stepped;

	Gripper() {
		setFocusPolicy(Qt::StrongFocus);
		setCursor(Qt::OpenHandCursor);
		const int line = fontMetrics().height();
		setFixedSize(line, line * 3 / 2);
	}

protected:
	void mousePressEvent(QMouseEvent *e) override {
		if (e->button() != Qt::LeftButton)
			return;
		setCursor(Qt::ClosedHandCursor);
		setFocus(Qt::MouseFocusReason);
	}
	void mouseMoveEvent(QMouseEvent *e) override {
		if (e->buttons() & Qt::LeftButton)
			dragged(e->globalPosition().toPoint());
	}
	void mouseReleaseEvent(QMouseEvent *) override { setCursor(Qt::OpenHandCursor); }
	void keyPressEvent(QKeyEvent *e) override {
		switch (e->key()) {
		case Qt::Key_Up: stepped(-1); break;
		case Qt::Key_Down: stepped(1); break;
		default: QWidget::keyPressEvent(e);
		}
	}
	void focusInEvent(QFocusEvent *) override { update(); }
	void focusOutEvent(QFocusEvent *) override { update(); }
	// 2 columns of 3 dots, the usual drag handle
	void paintEvent(QPaintEvent *) override {
		QPainter p(this);
		p.setRenderHint(QPainter::Antialiasing);
		p.setPen(Qt::NoPen);
		// Mid all but vanishes in some light styles
		QColor ink = palette().color(QPalette::WindowText);
		ink.setAlphaF(0.45F);
		p.setBrush(hasFocus() ? palette().color(QPalette::Highlight) : ink);
		const double dot = std::max(2.0, width() / 6.0);
		const QPointF mid(width() / 2.0, height() / 2.0);
		for (const int col : {-1, 1})
			for (const int row : {-1, 0, 1})
				p.drawEllipse(mid + QPointF(col * dot, row * dot * 2), dot / 2, dot / 2);
	}
};

// A graph list row's graph, read and drawn here with the window's look, so a
// change shows before Apply.
class LiveGraph final : public QWidget {
public:
	LiveGraph(Live &live, std::string name) : live(live), name(std::move(name)) {
		setFixedSize(0, fontMetrics().height() * 3 / 2);
		setMouseTracking(true);
	}

	// The graph's own width, as in the panel. A very wide one is squeezed to
	// fit the window, so its whole time span still shows.
	void fit() {
		own = int(live_width(live, name));
		const int most = 512;
		setFixedWidth(std::min(own, most));
		update();
	}

	// after a tick, so a tip left up keeps up, as the panel's does. One the
	// tick didn't read would draw the same.
	void tick(bool read) {
		if (read)
			update();
		if (underMouse() && QToolTip::isVisible())
			showTip(QCursor::pos());
	}

protected:
	void paintEvent(QPaintEvent *) override {
		const auto [w, h, scale] = drawn();
		if (w <= 0 || h <= 0)
			return;
		auto px = live_draw(live, name, size_t(w), size_t(h), size_t(scale));
		if (px.size() != size_t(w) * size_t(h))
			return;
		// cairo's ARGB32 is Qt's premultiplied one
		const QImage img(reinterpret_cast<const uchar *>(px.data()), w, h, qsizetype(w) * 4,
			QImage::Format_ARGB32_Premultiplied);
		QPainter p(this);
		p.drawImage(rect(), img);
		if (!framePending && live_easing(live, name)) {
			framePending = true;
			QTimer::singleShot(33, this, [this] {
				framePending = false;
				update();
			});
		}
	}
	void mouseMoveEvent(QMouseEvent *e) override {
		point(e->position().x());
		if (QToolTip::isVisible())
			showTip(e->globalPosition().toPoint());
	}
	void leaveEvent(QEvent *) override {
		if (live_leave(live))
			update();
	}
	bool event(QEvent *e) override {
		if (e->type() != QEvent::ToolTip)
			return QWidget::event(e);
		auto *help = static_cast<QHelpEvent *>(e);
		point(help->pos().x());
		showTip(help->globalPos());
		return true;
	}

private:
	// device pixels, as drawn before a squeeze to fit the window
	std::tuple<int, int, int> drawn() const {
		const double dpr = devicePixelRatioF();
		return {int(std::lround(own * dpr)), int(std::lround(height() * dpr)), int(std::max(1L, std::lround(dpr)))};
	}
	// the hover line goes where the pointer is in the drawn graph, squeezed or not
	void point(double x) {
		const int w = std::get<0>(drawn());
		if (w <= 0 || width() <= 0)
			return;
		const auto col = size_t(std::max(0.0, std::floor(x * w / width())));
		if (live_hover(live, name, col, size_t(w)))
			update();
	}
	void showTip(QPoint at) {
		const auto [w, h, scale] = drawn();
		QToolTip::showText(at, qs(wrap_tip(live_tip(live, name, size_t(w), size_t(h), size_t(scale)))), this);
	}

	Live &live;
	std::string name;
	int own = 0;
	bool framePending = false;
};

// counts the style changes on what it watches
class StyleCount final : public QObject {
public:
	int n = 0;

protected:
	bool eventFilter(QObject *o, QEvent *e) override {
		n += int(e->type() == QEvent::StyleChange);
		return QObject::eventFilter(o, e);
	}
};

// Qt sends a change event for a tip set to what it was, and lays the grid
// out again for a height set to what it was, and a refresh sets most to
// what they were.
void setTip(QWidget *w, const QString &tip) {
	if (w->toolTip() != tip)
		w->setToolTip(tip);
}

void setMinHeight(QGridLayout *grid, int row, int height) {
	if (grid->rowMinimumHeight(row) != height)
		grid->setRowMinimumHeight(row, height);
}

// counts the events of one type that reach what it watches
class EventCount final : public QObject {
public:
	explicit EventCount(QEvent::Type type) : type(type) {}
	QEvent::Type type;
	int n = 0;

protected:
	bool eventFilter(QObject *o, QEvent *e) override {
		n += int(e->type() == type);
		return QObject::eventFilter(o, e);
	}
};

struct Row {
	RowInfo info;
	QLabel *label = nullptr;
	QCheckBox *check = nullptr;
	TrackSlider *slider = nullptr;
	QDoubleSpinBox *spin = nullptr;
	QComboBox *combo = nullptr;
	QLineEdit *line = nullptr;
	QPushButton *chip = nullptr;
	QToolButton *revert = nullptr;
	QWidget *control = nullptr;
	QLabel *head = nullptr;
	QFrame *rule = nullptr;
	QGridLayout *grid = nullptr;
	int gap = -1;
	// in a section's column, where lineUpSections sets the gap
	bool sided = false;
	// as the last refresh read it, for the graph list's revert
	bool canReset = false;
	std::vector<QRadioButton *> radios;
	// what a dropdown's items stand for, which can change as themes are saved
	std::vector<std::string> keys;
	// the color the chip was last styled with
	QString chipColor;
	std::string id() const { return std::string(info.id); }
};

class Window final : public QDialog {
public:
	explicit Window(Dialog &d) : d(d), live(live_new(d)), tabList(new QListWidget), pages(new QStackedWidget) {
		setWindowTitle(qs(dialog_title(d)));
		auto tabs = dialog_tabs(d);
		for (auto &r : dialog_rows(d)) {
			auto row = std::make_unique<Row>();
			row->info = r;
			rows.push_back(std::move(row));
		}

		tabList->setAccessibleName("Pages");
		shade(this, false);
		// before the selection, which is tinted from its Base
		shade(tabList, true);
		softenSelection(tabList);

		auto hasRows = [this](int tab) {
			return std::ranges::any_of(rows, [tab](auto &r) { return r->info.tab == tab; });
		};
		// knobs lists a parent before its tabs, so one pass finds every stack
		std::vector<QStackedWidget *> stackOf(tabs.size(), nullptr);
		for (size_t i = 0; i < tabs.size(); i++) {
			bool parent = false;
			for (auto &t : tabs)
				parent |= t.parent == int(i);
			int depth = 0;
			for (int p = tabs[i].parent; p >= 0; p = tabs[size_t(p)].parent)
				depth++;
			tabList->addItem(new QListWidgetItem(QString(qsizetype(depth) * 4, ' ') + qs(tabs[i].label)));
			lineTab.push_back(int(i));
			lineUp.push_back(tabs[i].parent);
			leaf.push_back(!parent);
			// a tab with rows of its own keeps them on top of the tabs under it
			const int up = tabs[i].parent;
			if (up >= 0 && stackOf[size_t(up)]) {
				QStackedWidget *stack = stackOf[size_t(up)];
				tabPage.push_back(tabPage[size_t(up)]);
				subPage.push_back({stack, stack->addWidget(buildPage(int(i), tabs[i], true))});
				continue;
			}
			if (parent && !hasRows(int(i))) {
				tabPage.push_back(-1);
				subPage.push_back({});
				continue;
			}
			QWidget *page = buildPage(int(i), tabs[i], !parent);
			subPage.push_back({});
			if (parent) {
				stackOf[i] = new QStackedWidget;
				subPage.back() = {.stack = stackOf[i], .index = 0};
				// no rule under the parent's rows: the shade change marks the edge
				auto *holder = new QWidget;
				auto *v = new QVBoxLayout(holder);
				v->setContentsMargins(0, 0, 0, 0);
				v->addWidget(page);
				v->addWidget(stackOf[i], 1);
				page = holder;
			}
			tabPage.push_back(pages->addWidget(page));
		}
		orderSections();
		const int tabWidth = tabList->sizeHintForColumn(0);
		tabList->setFixedWidth(tabWidth + (2 * tabList->frameWidth()) + 24);
		for (int i = 0; i < tabList->count(); i++)
			tabList->item(i)->setSizeHint(QSize(tabWidth, tabList->fontMetrics().height() * 2));
		// one width for every number box, so the sliders all end in one column
		int numberWidth = 0;
		for (auto &r : rows)
			if (r->spin)
				numberWidth = std::max(numberWidth, r->spin->sizeHint().width());
		for (auto &r : rows)
			if (r->spin)
				r->spin->setFixedWidth(numberWidth);
		// and one label width per side of the sections, so their colors line up
		for (size_t side = 0; side < 2; side++) {
			int labelWidth = 0;
			for (auto &r : rows)
				// a same_line row's label sits in the row it joins
				if (r->label && !r->info.section.empty() && !r->info.same_line && size_t(r->info.beside) == side)
					labelWidth = std::max(labelWidth, r->label->sizeHint().width());
			for (auto &s : sections) {
				s.side[side]->setColumnMinimumWidth(0, labelWidth);
				if (side == 0 && s.lead)
					s.lead->setColumnMinimumWidth(0, labelWidth);
			}
		}
		connect(tabList, &QListWidget::currentRowChanged, this, [this](int i) { showTab(i); });

		resetButton = new QPushButton("Reset history");
		resetButton->setAutoDefault(false);
		resetButton->setToolTip("Start every graph over now, in the panel too. With Keep across restarts on, the "
								"history saved across restarts is deleted too.");
		connect(resetButton, &QPushButton::clicked, this, [this] { resetHistory(); });

		// Cancel stands a little apart from the 2 that save
		auto *cancel = new QPushButton("Cancel");
		applyButton = new QPushButton("&Apply");
		auto *closeButton = new QPushButton("OK");
		cancel->setToolTip("Discard every change and close Settings.");
		applyButton->setToolTip("Save changes now, without closing Settings.");
		closeButton->setToolTip("Save changes and close Settings.");
		closeButton->setDefault(true);
		connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
		connect(applyButton, &QPushButton::clicked, this, [this] { save(); });
		connect(closeButton, &QPushButton::clicked, this, [this] {
			if (save())
				accept();
		});
		int gap = style()->layoutSpacing(QSizePolicy::PushButton, QSizePolicy::PushButton, Qt::Horizontal);
		if (gap < 0)
			gap = 6;
		auto *buttons = new QHBoxLayout;
		buttons->setSpacing(0);
		// under the pages, where Keep across restarts is
		buttons->addSpacing(tabList->minimumWidth() + gap);
		buttons->addWidget(resetButton);
		buttons->addStretch(1);
		buttons->addWidget(cancel);
		buttons->addSpacing(2 * gap);
		buttons->addWidget(applyButton);
		buttons->addSpacing(gap);
		buttons->addWidget(closeButton);
		bottom = {resetButton, cancel, applyButton, closeButton};

		auto *body = new QHBoxLayout;
		body->addWidget(tabList);
		body->addWidget(pages, 1);
		auto *outer = new QVBoxLayout(this);
		outer->addLayout(body, 1);
		outer->addSpacing(HEAD_GAP / 2);
		outer->addLayout(buttons);

		tabList->setCurrentRow(0);
		refresh();
		qApp->installEventFilter(this);
		if (list) {
			auto *tick = new QTimer(this);
			connect(tick, &QTimer::timeout, this, [this, tick] {
				tick->setInterval(int(live_sample(*live)));
				for (auto &l : listRows)
					l.space->tick(live_read(*live, l.name));
			});
			// the first read only primes, and a second one ms later is noise
			tick->start(int(live_sample(*live)));
		}
	}

	Window(const Window &) = delete;
	Window &operator=(const Window &) = delete;
	Window(Window &&) = delete;
	Window &operator=(Window &&) = delete;
	~Window() override {
		qApp->removeEventFilter(this);
		quiet(this);
	}

	// Enter is OK and Esc is Cancel, even from inside a field or on a checkbox.
	// A focused button takes Enter as a press, as in any dialog.
	bool eventFilter(QObject *obj, QEvent *ev) override {
		if (ev->type() != QEvent::KeyPress)
			return false;
		auto *w = qobject_cast<QWidget *>(obj);
		if (!w || w->window() != this)
			return false;
		const auto *k = static_cast<QKeyEvent *>(ev);
		const int key = k->key();
		if ((key == Qt::Key_Tab || key == Qt::Key_Backtab) && (k->modifiers() & Qt::ControlModifier)) {
			stepTab(key == Qt::Key_Backtab || (k->modifiers() & Qt::ShiftModifier) ? -1 : 1);
			return true;
		}
		// or the tab list takes them as a page jump, to its first or last line
		if ((key == Qt::Key_PageDown || key == Qt::Key_PageUp) && (k->modifiers() & Qt::ControlModifier)) {
			stepTab(key == Qt::Key_PageUp ? -1 : 1);
			return true;
		}
		if (key == Qt::Key_Escape) {
			reject();
			return true;
		}
		if (key != Qt::Key_Return && key != Qt::Key_Enter)
			return false;
		// the dialog would give Enter on a button that isn't auto default to OK
		if (qobject_cast<QPushButton *>(obj) || qobject_cast<QToolButton *>(obj)) {
			static_cast<QAbstractButton *>(obj)->click();
			return true;
		}
		// leaving a field is what commits a half-typed value
		if (QWidget *f = focusWidget())
			f->clearFocus();
		if (save())
			accept();
		return true;
	}

	void showEvent(QShowEvent *e) override {
		QDialog::showEvent(e);
		fitFrame();
	}
	void moveEvent(QMoveEvent *e) override {
		QDialog::moveEvent(e);
		fitFrame();
	}
	// Opened to fit the screen, the frame then pushed the buttons past its
	// bottom. So once the frame is known, take it off, and keep it on screen.
	void fitFrame() {
		const QMargins frame = windowHandle() ? windowHandle()->frameMargins() : QMargins();
		const QScreen *sc = screen();
		if (framed || frame.isNull() || !sc)
			return;
		framed = true;
		const QRect room = sc->availableGeometry();
		resize(fitSize(size(), room, frame));
		const QSize full = size().grownBy(frame);
		const int x = std::max(room.left(), std::min(pos().x(), room.x() + room.width() - full.width()));
		const int y = std::max(room.top(), std::min(pos().y(), room.y() + room.height() - full.height()));
		if (QPoint(x, y) != pos())
			move(x, y);
	}

	int tabCount() const { return tabList->count(); }
	std::string tabText(int i) const { return tabList->item(i)->text().toStdString(); }
	std::vector<std::string> dimLines() const {
		std::vector<std::string> out;
		for (int i = 0; i < tabList->count(); i++)
			if (tabList->item(i)->background().style() != Qt::NoBrush)
				out.push_back(tabList->item(i)->text().trimmed().toStdString());
		return out;
	}
	std::vector<std::string> shownRows() const {
		std::vector<std::string> ids;
		for (const auto &r : rows) {
			QWidget *w = r->control ? r->control : r->label;
			if (w && w->isVisible())
				ids.push_back(r->id());
		}
		return ids;
	}
	std::vector<std::string> buttonTexts() const {
		std::vector<std::string> out;
		out.reserve(bottom.size());
		for (auto *b : bottom)
			out.push_back(b->text().toStdString());
		return out;
	}
	// the first color box, or the first whole number box at 10 or more that
	// isn't greyed
	QWidget *firstBox(bool number) const {
		for (const auto &r : rows) {
			if (number && r->spin && r->spin->isEnabled() && r->info.whole && r->spin->value() >= 10)
				return r->spin;
			if (!number && r->chip)
				return r->line;
		}
		return nullptr;
	}
	const Row *firstSlider() const {
		const auto r = std::ranges::find_if(rows, [](auto &row) { return row->slider != nullptr; });
		return r == rows.end() ? nullptr : r->get();
	}
	// the most common color in the page that scrolls, in the tab list, and in
	// the rest of the window, as drawn
	std::tuple<QColor, QColor, QColor> shadesNow() {
		const QImage img = grab().toImage();
		QRect inside;
		QRect area;
		for (auto *s : findChildren<QScrollArea *>())
			if (s->isVisible()) {
				inside = QRect(s->viewport()->mapTo(this, QPoint()), s->viewport()->size());
				area = QRect(s->mapTo(this, QPoint()), s->size());
			}
		const QRect tabs(tabList->mapTo(this, QPoint()), tabList->size());
		const QRect tabsInside(tabList->viewport()->mapTo(this, QPoint()), tabList->viewport()->size());
		std::map<QRgb, int> in;
		std::map<QRgb, int> list;
		std::map<QRgb, int> out;
		for (int y = 0; y < img.height(); y++)
			for (int x = 0; x < img.width(); x++) {
				const QPoint at(x, y);
				if (inside.contains(at))
					in[img.pixel(at)]++;
				else if (tabsInside.contains(at))
					list[img.pixel(at)]++;
				else if (!area.contains(at) && !tabs.contains(at))
					out[img.pixel(at)]++;
			}
		auto most = [](const std::map<QRgb, int> &seen) {
			auto top = std::ranges::max_element(seen, [](auto &a, auto &b) { return a.second < b.second; });
			return top == seen.end() ? QColor() : QColor(top->first);
		};
		return {most(in), most(list), most(out)};
	}
	bool applyOn() const { return applyButton->isEnabled(); }
	void commitTyped() { takeTyped(); }
	bool isLeaf(int i) const { return leaf[size_t(lineTab[size_t(i)])]; }
	void select(int i) { tabList->setCurrentRow(i); }

	// Neither can be undone, so No is the default (style guide, Message boxes).
	bool askDelete(const QString &name) {
		return QMessageBox::question(this, "Delete theme", QString("Delete the theme %1?").arg(name),
				   QMessageBox::Yes | QMessageBox::No, QMessageBox::No) == QMessageBox::Yes;
	}

	bool askReset() {
		return QMessageBox::question(this, "Reset history",
				   "Keep across restarts is on for this or another panel item, so this deletes saved "
				   "history, not only what's on screen.\n\nReset history?",
				   QMessageBox::Yes | QMessageBox::No, QMessageBox::No) == QMessageBox::Yes;
	}

	std::string selectedText() const { return tabList->currentItem()->text().toStdString(); }
	void moveGraph(int from, int to) { moveTo(from, to); }
	std::unique_ptr<ColorPicker> firstPicker() {
		for (auto &r : rows)
			if (r->chip)
				return picker(*r);
		return nullptr;
	}
	// chips restyled by a refresh with nothing changed, then by one after the
	// first color row is set to `color`
	std::vector<int> chipRestyles(const std::string &color) {
		StyleCount count;
		for (auto &r : rows)
			if (r->chip)
				r->chip->installEventFilter(&count);
		refresh();
		const int still = std::exchange(count.n, 0);
		const auto r = std::ranges::find_if(rows, [](auto &row) { return row->chip != nullptr; });
		if (r != rows.end())
			set_text(d, (*r)->id(), color);
		refresh();
		return {still, count.n};
	}

	// tips set and layouts asked for by a refresh with nothing changed, on
	// each page in turn, then the tips set by one after setting `id` to
	// `number`
	std::vector<int> refreshChurn(const std::string &id, double number) {
		EventCount tips(QEvent::ToolTipChange);
		EventCount layouts(QEvent::LayoutRequest);
		// on each widget, since an app's filters see only the first app's thread
		QList<QWidget *> all = findChildren<QWidget *>();
		all.push_back(this);
		for (QWidget *w : all) {
			w->installEventFilter(&tips);
			w->installEventFilter(&layouts);
		}
		int stillTips = 0;
		int stillLayouts = 0;
		for (int i = 0; i < tabCount(); i++) {
			select(i);
			// a layout can ask for another as it settles
			for (int k = 0; k < 5; k++)
				QApplication::processEvents();
			tips.n = layouts.n = 0;
			refresh();
			QApplication::processEvents();
			stillTips += tips.n;
			stillLayouts += layouts.n;
		}
		tips.n = 0;
		set_number(d, id, number);
		refresh();
		QApplication::processEvents();
		return {stillTips, stillLayouts, tips.n};
	}

	// microseconds per call, of `times` refreshes and then `times` steps of
	// the first slider with the events they post
	std::vector<double> refreshTimes(int times) {
		std::vector<double> out;
		out.reserve(size_t(times) * 2);
		QElapsedTimer t;
		for (int i = 0; i < times; i++) {
			t.start();
			refresh();
			out.push_back(double(t.nsecsElapsed()) / 1000);
		}
		const auto r = std::ranges::find_if(rows, [](auto &row) { return row->slider != nullptr; });
		for (int i = 0; r != rows.end() && i < times; i++) {
			t.start();
			(*r)->slider->setValue(((*r)->slider->value() + (SLIDER_STEPS / 7)) % SLIDER_STEPS);
			QApplication::processEvents();
			out.push_back(double(t.nsecsElapsed()) / 1000);
		}
		return out;
	}

	std::vector<std::string> greyed() const {
		std::vector<std::string> ids;
		for (const auto &r : rows)
			if (r->control && !r->control->isEnabled())
				ids.push_back(r->id());
		return ids;
	}

	// the graph list row or Colors section `w` sits in, or ""
	std::string groupOf(const QWidget *w) const {
		for (const auto &l : listRows)
			for (const QWidget *c : l.cells())
				if (c == w || c->isAncestorOf(w))
					return l.name;
		for (const auto &s : sections)
			if (s.box->isAncestorOf(w))
				return s.graph;
		return {};
	}
	// where Tab starts in the graph list, or on Colors
	QWidget *firstStop(bool colors) const {
		if (!colors)
			return listRows.empty() ? nullptr : listRows[0].show->check;
		for (int i = 0; sectionList && i < sectionList->count(); i++)
			for (auto *w : sectionList->itemAt(i)->widget()->findChildren<QWidget *>())
				if ((w->focusPolicy() & Qt::TabFocus) && !w->focusProxy() && w->isVisible() && w->isEnabled())
					return w;
		return nullptr;
	}

	// graph width and x, and the width box's x, per graph list row
	std::vector<int> listPlaces() const {
		std::vector<int> out;
		for (const auto &l : listRows)
			for (const int v : {l.space->width(), l.space->x(), l.width->control->x()})
				out.push_back(v);
		return out;
	}

	// "graph left right" per section from the top, as sections_after_move gives
	std::vector<std::string> sectionPlaces() const {
		std::vector<std::pair<int, std::string>> found;
		for (const auto &s : sections) {
			int left = 0;
			int right = INT_MAX;
			for (const auto &r : rows) {
				// the lead line spans both columns
				if (r->info.section != s.graph || r.get() == s.title || (r->grid && r->grid == s.lead))
					continue;
				for (QWidget *w : {static_cast<QWidget *>(r->label), r->control, static_cast<QWidget *>(r->revert),
						 static_cast<QWidget *>(r->head)}) {
					if (!w || !w->isVisibleTo(s.box))
						continue;
					const int x = w->mapTo(s.box, QPoint()).x();
					if (r->info.beside)
						right = std::min(right, x);
					else
						left = std::max(left, x + w->width());
				}
			}
			found.emplace_back(s.box->y(), s.graph + " " + std::to_string(left) + " " + std::to_string(right));
		}
		std::ranges::sort(found);
		std::vector<std::string> out;
		out.reserve(found.size());
		for (auto &f : found)
			out.push_back(f.second);
		return out;
	}

	// "id left mid control right revert" per row shown with a label, in the
	// window: its label's left edge and middle line, its control's left edge,
	// the right edge of its last part, such as a color's hex box, and its
	// revert's left edge
	std::vector<std::string> rowSpots() const {
		std::vector<std::string> out;
		for (const auto &r : rows) {
			if (!r->label || !r->label->isVisible())
				continue;
			const QRect l(r->label->mapTo(this, QPoint(0, 0)), r->label->size());
			const int c = r->control->mapTo(this, QPoint(0, 0)).x();
			const QWidget *last = r->line ? static_cast<QWidget *>(r->line) : r->control;
			if (!r->radios.empty())
				last = r->radios.back();
			const int right = last->mapTo(this, QPoint(last->width(), 0)).x();
			const int back = r->revert ? r->revert->mapTo(this, QPoint(0, 0)).x() : -1;
			out.push_back(r->id() + " " + std::to_string(l.left()) + " " + std::to_string(l.center().y()) + " " +
				std::to_string(c) + " " + std::to_string(right) + " " + std::to_string(back));
		}
		return out;
	}

	// "name left mid ..." per graph list row, for each of its cells in order
	std::vector<std::string> listCells() const {
		std::vector<std::string> out;
		for (const auto &l : listRows) {
			std::string line = l.name;
			for (const QWidget *c : l.cells()) {
				const QRect at(c->mapTo(this, QPoint(0, 0)), c->size());
				line += " " + std::to_string(at.left()) + " " + std::to_string(at.center().y());
			}
			out.push_back(line);
		}
		return out;
	}

	// "graph left right" per section: how far down its box the left column's
	// first row under the colors mode starts, and the right column's first
	std::vector<std::string> sectionTops() const {
		std::vector<std::string> out;
		for (const auto &s : sections) {
			std::array<int, 2> top{-1, -1};
			for (const auto &r : rows) {
				if (r->info.section != s.graph || r.get() == s.title)
					continue;
				const size_t side = r->info.beside ? 1 : 0;
				int y = INT_MAX;
				for (QWidget *w : {static_cast<QWidget *>(r->head), static_cast<QWidget *>(r->label), r->control,
						 static_cast<QWidget *>(r->revert)})
					if (w && w->isVisibleTo(s.box))
						y = std::min(y, w->mapTo(s.box, QPoint()).y());
				if (y == INT_MAX)
					continue;
				// the mode and what joins its line sit above both columns
				if (r->grid && r->grid == s.lead)
					continue;
				if (top[side] < 0)
					top[side] = y;
			}
			out.push_back(s.graph + " " + std::to_string(top[0]) + " " + std::to_string(top[1]));
		}
		return out;
	}

	// the shown page's scroll area, as "view content": the height it shows and
	// the height its page asks for
	std::string scrollNow() const {
		for (auto *s : findChildren<QScrollArea *>())
			if (s->isVisible())
				return std::to_string(s->viewport()->height()) + " " + std::to_string(s->widget()->sizeHint().height());
		return "0 0";
	}

private:
	Dialog &d;
	rust::Box<Live> live;
	bool framed = false;
	std::vector<std::unique_ptr<Row>> rows;
	// per tab of the spec
	std::vector<int> tabPage;
	std::vector<bool> leaf;
	// the spec's tab on each line of the tab list, and the line over it
	std::vector<int> lineTab;
	std::vector<int> lineUp;
	struct SubPage {
		QStackedWidget *stack = nullptr;
		int index = 0;
	};
	std::vector<SubPage> subPage;
	QPushButton *saveTheme = nullptr;
	QPushButton *renameTheme = nullptr;
	QPushButton *deleteTheme = nullptr;
	QListWidget *tabList = nullptr;
	QStackedWidget *pages = nullptr;
	QPushButton *applyButton = nullptr;
	QPushButton *resetButton = nullptr;
	std::vector<QPushButton *> bottom;

	// One graph per row, first drawn first, in the order they're placed.
	struct ListRow {
		std::string name;
		Row *show = nullptr;
		Row *width = nullptr;
		Row *sample = nullptr;
		Row *time = nullptr;
		Row *vertical = nullptr;
		LiveGraph *space = nullptr;
		QToolButton *revert = nullptr;
		Gripper *grip = nullptr;
		std::array<QWidget *, 8> cells() const {
			return {show->control, space, width->control, sample->control, time->control, vertical->control, revert,
				grip};
		}
	};
	QWidget *list = nullptr;
	QGridLayout *listGrid = nullptr;
	std::vector<ListRow> listRows;

	// a tab with tabs under it shows its first one, as listed
	void showTab(int i) {
		if (i < 0)
			return;
		for (int j = i; j < tabList->count(); j++) {
			const auto t = size_t(lineTab[size_t(j)]);
			if (tabPage[t] < 0)
				continue;
			pages->setCurrentIndex(tabPage[t]);
			SubPage s = subPage[t];
			int shown = j;
			if (!leaf[t] && j + 1 < tabList->count())
				if (const SubPage &next = subPage[size_t(lineTab[size_t(j) + 1])]; s.stack && next.stack == s.stack) {
					s = next;
					shown = j + 1;
				}
			if (s.stack)
				s.stack->setCurrentIndex(s.index);
			lightAlong(i, shown);
			return;
		}
	}

	// Ctrl+Tab goes page by page, as a tab bar does, so a parent that only
	// shows its first sub-tab isn't a stop of its own.
	void stepTab(int by) {
		const int n = tabList->count();
		for (int i = tabList->currentRow(), left = n; left > 0; left--) {
			i = (i + by + n) % n;
			if (isLeaf(i)) {
				tabList->setCurrentRow(i);
				return;
			}
		}
	}

	// The line picked keeps the highlight. The sub-tab it shows and the tabs
	// over either get a dimmer one, so Themes stays lit under a sub-tab.
	void lightAlong(int picked, int shown) {
		std::vector<bool> on(size_t(tabList->count()), false);
		for (const int k : {picked, shown})
			for (int up = k; up >= 0; up = lineUp[size_t(up)])
				on[size_t(up)] = true;
		on[size_t(picked)] = false;
		const QPalette pal = tabList->palette();
		const QColor base = pal.color(QPalette::Base);
		const QColor pick = pal.color(QPalette::Highlight);
		auto mix = [](float a, float b) { return a + ((b - a) * 0.45F); };
		const QBrush dim(QColor::fromRgbF(mix(base.redF(), pick.redF()), mix(base.greenF(), pick.greenF()),
			mix(base.blueF(), pick.blueF())));
		for (int k = 0; k < tabList->count(); k++)
			tabList->item(k)->setBackground(on[size_t(k)] ? dim : QBrush());
	}

	// One graph's colors on a page of them: its own rows on the left, and the
	// ones beside them on the right.
	struct Section {
		std::string graph;
		QWidget *box = nullptr;
		Row *title = nullptr;
		QGridLayout *outer = nullptr;
		// the first row on the left, its colors mode, over both columns
		QGridLayout *lead = nullptr;
		int leadLine = 0;
		bool leadDone = false;
		std::array<QGridLayout *, 2> side{};
		std::array<int, 2> line{};
	};
	std::vector<Section> sections;
	QVBoxLayout *sectionList = nullptr;
	// the last row placeRow put on a line, for a same_line row to join
	Row *lastPlaced = nullptr;

	// Sections follow the graph order, before Apply too.
	void orderSections() {
		if (!sectionList)
			return;
		std::vector<QWidget *> want;
		for (auto &name : section_order(d))
			for (auto &s : sections)
				if (s.graph == std::string(name))
					want.push_back(s.box);
		// one with no graph in the list still has to sit somewhere
		for (auto &s : sections)
			if (std::ranges::find(want, s.box) == want.end())
				want.push_back(s.box);
		std::vector<QWidget *> now;
		now.reserve(size_t(sectionList->count()));
		for (int i = 0; i < sectionList->count(); i++)
			now.push_back(sectionList->itemAt(i)->widget());
		if (now == want)
			return;
		for (auto *w : want)
			sectionList->removeWidget(w);
		for (auto *w : want)
			sectionList->addWidget(w);
		tabInOrder(want);
	}

	// Whatever the mode shows first on the left sits level with the top of
	// the right column, Background. So the first thing shown in a column
	// has no gap over it, and a heading's gap goes over both columns.
	void lineUpSections() {
		for (auto &s : sections) {
			std::array<bool, 2> above{};
			bool headFirst = false;
			for (auto &rp : rows) {
				const Row &r = *rp;
				size_t side = 2;
				if (r.grid == s.side[0])
					side = 0;
				else if (r.grid == s.side[1])
					side = 1;
				if (side == 2)
					continue;
				QWidget *w = r.head ? static_cast<QWidget *>(r.head) : r.label;
				if (!w || w->isHidden())
					continue;
				if (side == 0 && !above[0])
					headFirst = r.head != nullptr;
				if (r.head && r.gap >= 0)
					setMinHeight(r.grid, r.gap, above[side] ? HEAD_GAP : 0);
				above[side] = true;
			}
			setMinHeight(s.outer, 3, s.lead && headFirst ? HEAD_GAP : 0);
		}
	}

	// A section's colors are short, so the room goes after its reverts,
	// which then sit by what they reset.
	static QGridLayout *rowGrid(QWidget *on, bool section = false) {
		auto *grid = new QGridLayout(on);
		grid->setColumnStretch(section ? 3 : 1, 1);
		grid->setVerticalSpacing(6);
		return grid;
	}

	// The section a row goes in, made on its first row, which titles it when
	// it's a heading.
	Section &sectionOf(Row &r, QGridLayout *grid, int &line) {
		for (auto &s : sections)
			if (s.graph == std::string(r.info.section))
				return s;
		if (!sectionList) {
			auto *holder = new QWidget;
			sectionList = new QVBoxLayout(holder);
			sectionList->setContentsMargins(0, 0, 0, 0);
			sectionList->setSpacing(HEAD_GAP);
			grid->addWidget(holder, line++, 0, 1, 3);
		}
		Section s{.graph = std::string(r.info.section)};
		s.box = new QWidget;
		auto *g = new QGridLayout(s.box);
		s.outer = g;
		g->setContentsMargins(0, 0, 0, 0);
		g->setColumnStretch(0, 1);
		g->setColumnStretch(1, 1);
		g->setHorizontalSpacing(3 * HEAD_GAP / 2);
		if (r.info.control == ControlKind::Heading) {
			auto *h = new QLabel(qs(r.info.label));
			QFont f = h->font();
			f.setBold(true);
			f.setPointSizeF(f.pointSizeF() * 1.15);
			h->setFont(f);
			auto *rule = new QFrame;
			rule->setFrameShape(QFrame::HLine);
			rule->setFrameShadow(QFrame::Sunken);
			g->addWidget(h, 0, 0, 1, 2);
			g->addWidget(rule, 1, 0, 1, 2);
			r.head = h;
			r.rule = rule;
			s.title = &r;
		}
		for (int side = 0; side < 2; side++) {
			auto *col = new QWidget;
			s.side[size_t(side)] = rowGrid(col, true);
			s.side[size_t(side)]->setContentsMargins(0, 0, 0, 0);
			g->addWidget(col, 4, side, Qt::AlignTop);
		}
		sectionList->addWidget(s.box);
		sections.push_back(s);
		return sections.back();
	}

	QWidget *buildPage(int tab, const TabInfo &info, bool scrolls) {
		const size_t sectionsBefore = sections.size();
		auto *page = new QWidget;
		auto *grid = rowGrid(page);
		int line = 0;
		for (auto &rp : rows) {
			Row &r = *rp;
			if (r.info.tab != tab)
				continue;
			if (r.info.in_list) {
				if (!list)
					grid->addWidget(buildList(), line++, 0, 1, 3);
				continue;
			}
			if (r.info.section.empty()) {
				placeRow(r, info, grid, line);
				continue;
			}
			Section &s = sectionOf(r, grid, line);
			if (s.title == &r)
				continue;
			const size_t side = r.info.beside ? 1 : 0;
			if (side == 0 && !std::exchange(s.leadDone, true) && r.info.control != ControlKind::Heading) {
				auto *top = new QWidget;
				s.lead = rowGrid(top, true);
				s.lead->setContentsMargins(0, 0, 0, 0);
				s.outer->addWidget(top, 2, 0, 1, 2, Qt::AlignTop);
				placeRow(r, info, s.lead, s.leadLine);
				continue;
			}
			placeRow(r, info, s.side[side], s.line[side], true);
		}
		if (!scrolls)
			return page;
		grid->setRowStretch(line, 1);
		auto *scroll = new FitScroll;
		scroll->ownHeight = sections.size() == sectionsBefore;
		scroll->setWidget(page);
		scroll->setWidgetResizable(true);
		scroll->setFrameShape(QFrame::NoFrame);
		scroll->setAccessibleName(qs(info.label));
		shade(scroll, true);
		return scroll;
	}

	// A heading gets a gap over it, which a column's first one in a section
	// keeps for when a mode hides what's over it.
	void placeRow(Row &r, const TabInfo &info, QGridLayout *grid, int &line, bool inSection = false) {
		const QString label = qs(r.info.label);
		r.grid = grid;
		r.sided = inSection;
		if (r.info.control == ControlKind::Heading) {
			// a heading that only repeats the tab's name says nothing
			if (label == qs(info.label))
				return;
			auto *h = new QLabel(label);
			QFont f = h->font();
			f.setBold(true);
			h->setFont(f);
			auto *rule = new QFrame;
			rule->setFrameShape(QFrame::HLine);
			rule->setFrameShadow(QFrame::Sunken);
			if (line > 0 || inSection) {
				r.gap = line;
				grid->setRowMinimumHeight(line++, HEAD_GAP);
			}
			grid->addWidget(h, line++, 0, 1, -1);
			grid->addWidget(rule, line++, 0, 1, -1);
			r.head = h;
			r.rule = rule;
			lastPlaced = nullptr;
			return;
		}
		r.label = new QLabel(label);
		r.control = buildControl(r);
		r.revert = new QToolButton;
		r.revert->setIcon(QIcon::fromTheme("edit-undo", style()->standardIcon(QStyle::SP_BrowserReload)));
		r.revert->setAutoRaise(true);
		connect(r.revert, &QToolButton::clicked, this, [this, &r] {
			reset_row(d, r.id());
			refresh();
		});
		nameRow(r, spokenName(r));
		Row *prev = std::exchange(lastPlaced, &r);
		if (r.info.same_line && prev && prev->grid == grid) {
			// the row above keeps its label and revert, and shares its
			// control's cell with this row's 3
			auto *holder = new QWidget;
			auto *h = new QHBoxLayout(holder);
			h->setContentsMargins(0, 0, 0, 0);
			grid->removeWidget(prev->control);
			h->addWidget(prev->control);
			r.label->setContentsMargins(HEAD_GAP, 0, 6, 0);
			h->addWidget(r.label);
			h->addWidget(r.control);
			h->addWidget(r.revert);
			h->addStretch(1);
			grid->addWidget(holder, line - 1, 1);
			return;
		}
		r.label->setContentsMargins(16 * r.info.indent, 0, 12, 0);
		grid->addWidget(r.label, line, 0);
		grid->addWidget(r.control, line, 1);
		grid->addWidget(r.revert, line, 2);
		line++;
	}

	Row *rowFor(const rust::String &id) {
		for (auto &r : rows)
			if (r->info.id == id)
				return r.get();
		return nullptr;
	}

	// A row's label, after its graph's on Themes/Colors, where Low or Border
	// alone doesn't say whose.
	QString spokenName(const Row &r) const {
		QString label = qs(r.info.label);
		for (const auto &s : sections)
			if (s.graph == std::string(r.info.section) && s.title)
				return qs(s.title->info.label) + " " + label;
		return label;
	}

	// What a screen reader calls each of the row's controls, which have no
	// text of their own. It finds a label only beside what it labels, so the
	// buddy is the control, not a box inside it. A dropdown takes its label
	// from there, since Qt on Linux names it by its text.
	static void nameRow(Row &r, const QString &name) {
		if (r.label)
			r.label->setBuddy(r.control);
		for (QWidget *w : {static_cast<QWidget *>(r.slider), static_cast<QWidget *>(r.spin),
				 static_cast<QWidget *>(r.combo), static_cast<QWidget *>(r.line), r.control})
			if (w)
				w->setAccessibleName(name);
		if (r.check && r.check->text().isEmpty())
			r.check->setAccessibleName(name);
		if (r.chip)
			r.chip->setAccessibleName("Pick a color for " + name);
		if (r.revert)
			r.revert->setAccessibleName("Revert " + name);
	}

	// The rows under Graphs as one list: show, a space for the graph, width,
	// sample rate, time axis, vertical scale, revert and a gripper. The
	// grippers write the order.
	QWidget *buildList() {
		list = new QWidget;
		listGrid = new QGridLayout(list);
		listGrid->setContentsMargins(0, 0, 0, 0);
		listGrid->setColumnStretch(6, 1);
		for (auto &g : graph_rows(d)) {
			ListRow lr{.name = std::string(g.name),
				.show = rowFor(g.show),
				.width = rowFor(g.width),
				.sample = rowFor(g.sample),
				.time = rowFor(g.time),
				.vertical = rowFor(g.vertical)};
			if (!lr.show || !lr.width || !lr.sample || !lr.time || !lr.vertical)
				continue;
			for (Row *r : {lr.show, lr.width, lr.sample, lr.time, lr.vertical})
				r->control = buildControl(*r);
			const QString graph = qs(lr.show->info.label);
			lr.show->check->setText(graph);
			lr.space = new LiveGraph(*live, lr.name);
			lr.space->fit();
			lr.revert = new QToolButton;
			lr.revert->setIcon(QIcon::fromTheme("edit-undo", style()->standardIcon(QStyle::SP_BrowserReload)));
			lr.revert->setAutoRaise(true);
			lr.grip = new Gripper;
			lr.grip->setToolTip("Drag up or down to move this graph, or use Up and Down here. The top one is "
								"drawn first: on the left in a horizontal panel, on top in a vertical one.");
			nameRow(*lr.width, graph + " width %");
			nameRow(*lr.sample, graph + " sample %");
			nameRow(*lr.time, graph + " time axis");
			nameRow(*lr.vertical, graph + " vertical scale");
			lr.space->setAccessibleName(graph);
			lr.revert->setAccessibleName("Revert " + graph);
			lr.grip->setAccessibleName("Move " + graph);
			const std::string name = lr.name;
			connect(lr.revert, &QToolButton::clicked, this, [this, name] {
				for (auto &l : listRows)
					if (l.name == name)
						for (Row *r : {l.show, l.width, l.sample, l.time, l.vertical})
							reset_row(d, r->id());
				refresh();
			});
			lr.grip->dragged = [this, name](QPoint at) { dragTo(name, list->mapFromGlobal(at).y()); };
			lr.grip->stepped = [this, name](int by) { moveBy(name, by); };
			listRows.push_back(lr);
		}
		auto *widthHead = new QLabel("Width %");
		auto *sampleHead = new QLabel("Sample %");
		auto *timeHead = new QLabel("Time axis");
		auto *verticalHead = new QLabel("Vertical scale");
		// about the column, since a row's tip names its graph and how it was set
		widthHead->setToolTip("How wide each graph is, or how tall on a vertical panel, as a percentage of Width px "
							  "default, on the General tab.");
		sampleHead->setToolTip("How often each graph is read and redrawn, as a percentage of Sample ms default, on "
							   "the General tab. At 200% it's read half as often, and at 50% twice as often. Under "
							   "50 ms counts as 50 ms.");
		timeHead->setToolTip("How each graph spreads time across its width. Default follows Scale default under "
							 "Time axis, on the General tab, where the tip tells the curves apart.");
		verticalHead->setToolTip("How height goes up each graph. Global default follows Scale default under "
								 "Vertical axis, on the General tab, where the tip tells the scales apart.");
		listGrid->addWidget(widthHead, 0, 2);
		listGrid->addWidget(sampleHead, 0, 3);
		listGrid->addWidget(timeHead, 0, 4);
		listGrid->addWidget(verticalHead, 0, 5);
		placeList();
		return list;
	}

	// puts each row's widgets on its grid line, in listRows' order
	void placeList() {
		for (size_t i = 0; i < listRows.size(); i++) {
			const int at = int(i) + 1;
			const auto cells = listRows[i].cells();
			const std::array<int, 8> cols = {0, 1, 2, 3, 4, 5, 7, 8};
			for (size_t k = 0; k < cells.size(); k++) {
				listGrid->removeWidget(cells[k]);
				// narrower graphs sit left, so the column is the widest's
				listGrid->addWidget(cells[k], at, cols[k], k == 1 ? Qt::AlignLeft | Qt::AlignVCenter : Qt::Alignment());
			}
		}
		// the next drag event reads the new places
		listGrid->activate();
		std::vector<QWidget *> cells;
		cells.reserve(listRows.size() * 8);
		for (const auto &l : listRows)
			std::ranges::copy(l.cells(), std::back_inserter(cells));
		tabInOrder(cells);
	}

	// Moving a widget in a layout leaves Tab where it was made, so Tab goes
	// through `groups` in the order given, each keeping its own order inside.
	static void tabInOrder(const std::vector<QWidget *> &groups) {
		if (groups.empty())
			return;
		QWidget *top = groups.front()->window();
		std::vector<QWidget *> chain;
		for (QWidget *g : groups)
			for (QWidget *w = top->nextInFocusChain(); w != top; w = w->nextInFocusChain())
				if ((w->focusPolicy() & Qt::TabFocus) && !w->focusProxy() && (w == g || g->isAncestorOf(w)))
					chain.push_back(w);
		for (size_t i = 1; i < chain.size(); i++)
			setTabOrder(chain[i - 1], chain[i]);
	}

	int listIndex(const std::string &name) const {
		for (size_t i = 0; i < listRows.size(); i++)
			if (listRows[i].name == name)
				return int(i);
		return -1;
	}

	void moveTo(int from, int to) {
		if (from < 0 || to < 0 || from == to || !move_graph(d, size_t(from), size_t(to)))
			return;
		refresh();
	}

	void moveBy(const std::string &name, int by) {
		const int from = listIndex(name);
		moveTo(from, std::clamp(from + by, 0, int(listRows.size()) - 1));
	}

	// `y` is in the list's own coordinates
	void dragTo(const std::string &name, int y) {
		int to = 0;
		for (size_t i = 1; i < listRows.size(); i++)
			if (y >= listGrid->cellRect(int(i) + 1, 0).top())
				to = int(i);
		moveTo(listIndex(name), to);
	}

	// after a drag, a revert or a reload of the order
	void refreshList() {
		std::vector<ListRow> sorted;
		for (auto &g : graph_rows(d)) {
			const int i = listIndex(std::string(g.name));
			if (i >= 0)
				sorted.push_back(listRows[size_t(i)]);
		}
		bool moved = sorted.size() != listRows.size();
		for (size_t i = 0; !moved && i < sorted.size(); i++)
			moved = sorted[i].name != listRows[i].name;
		if (moved) {
			listRows = sorted;
			placeList();
		}
		live_look(*live, d);
		for (auto &l : listRows) {
			l.space->fit();
			bool changed = false;
			for (Row *r : {l.show, l.width, l.sample, l.time, l.vertical})
				changed |= r->canReset;
			l.revert->setEnabled(changed);
			setTip(l.revert, changed ? "Back to the defaults." : "");
		}
	}

	QDoubleSpinBox *numberBox(Row &r) {
		auto *spin = new QDoubleSpinBox;
		spin->setDecimals(r.info.whole ? 0 : 2);
		spin->setRange(std::isfinite(r.info.min) ? r.info.min : -1e9, std::isfinite(r.info.max) ? r.info.max : 1e9);
		spin->setKeyboardTracking(false);
		connect(spin, &QDoubleSpinBox::valueChanged, this, [this, &r](double v) {
			set_number(d, r.id(), v);
			refresh();
		});
		// with keyboard tracking off, the box itself says nothing until it's left
		connect(spin->findChild<QLineEdit *>(), &QLineEdit::textEdited, this, [this] { updateApply(); });
		return spin;
	}

	QWidget *buildControl(Row &r) {
		switch (r.info.control) {
		case ControlKind::Checkbox: {
			r.check = new QCheckBox;
			connect(r.check, &QCheckBox::toggled, this, [this, &r](bool on) {
				set_bool(d, r.id(), on);
				refresh();
			});
			return r.check;
		}
		case ControlKind::Slider: {
			r.slider = new TrackSlider;
			r.slider->setRange(0, SLIDER_STEPS);
			r.slider->setMinimumWidth(180);
			// as tall as the number box beside it, and the handle with it
			r.slider->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
			connect(r.slider, &QSlider::valueChanged, this, [this, &r](int v) {
				set_slider(d, r.id(), double(v) / SLIDER_STEPS);
				refresh();
			});
			r.spin = numberBox(r);
			auto *w = new QWidget;
			auto *h = new QHBoxLayout(w);
			h->setContentsMargins(0, 0, 0, 0);
			h->addWidget(r.slider, 1);
			h->addWidget(r.spin);
			return w;
		}
		case ControlKind::Number: {
			r.spin = numberBox(r);
			return r.spin;
		}
		case ControlKind::Radio: {
			auto *w = new QWidget;
			auto *h = new QHBoxLayout(w);
			h->setContentsMargins(0, 0, 0, 0);
			auto *group = new QButtonGroup(w);
			for (size_t i = 0; i < r.info.options.size(); i++) {
				auto *b = new QRadioButton(qs(r.info.option_labels[i]));
				group->addButton(b);
				r.radios.push_back(b);
				r.keys.push_back(std::string(r.info.options[i]));
				connect(b, &QRadioButton::toggled, this, [this, &r, i](bool on) {
					// the one turned off says so too, and the other one covers it
					if (!on)
						return;
					set_text(d, r.id(), r.keys[i]);
					refresh();
				});
				h->addWidget(b);
			}
			h->addStretch(1); // NOLINT(clang-analyzer-cplusplus.NewDeleteLeaks): w is group's Qt parent
			return w;
		}
		case ControlKind::Dropdown: {
			r.combo = new QComboBox;
			r.combo->setSizeAdjustPolicy(QComboBox::AdjustToContents);
			connect(r.combo, &QComboBox::activated, this, [this, &r](int i) {
				if (i >= 0 && size_t(i) < r.keys.size())
					set_text(d, r.id(), r.keys[size_t(i)]);
				refresh();
			});
			if (!r.info.chooser)
				return r.combo;
			return themeButtons(r.combo);
		}
		// a heading is drawn by placeRow, and has no control
		case ControlKind::Heading:
		case ControlKind::Text:
		case ControlKind::File:
		case ControlKind::Color: break;
		}
		r.line = new QLineEdit;
		connect(r.line, &QLineEdit::editingFinished, this, [this, &r] {
			set_text(d, r.id(), r.line->text().toStdString());
			refresh();
		});
		connect(r.line, &QLineEdit::textEdited, this, [this] { updateApply(); });
		if (r.info.control != ControlKind::Color)
			return r.line;

		r.line->setValidator(new QRegularExpressionValidator(QRegularExpression("#?[0-9a-fA-F]{0,6}"), r.line));
		r.line->setMaximumWidth(r.line->fontMetrics().horizontalAdvance("#DDDDDDD") + 16);
		r.chip = new QPushButton;
		r.chip->setFixedSize(36, r.line->sizeHint().height());
		r.chip->setToolTip("Pick a color.");
		connect(r.chip, &QPushButton::clicked, this, [this, &r] { pickColor(r); });
		auto *w = new QWidget;
		auto *h = new QHBoxLayout(w);
		h->setContentsMargins(0, 0, 0, 0);
		h->addWidget(r.chip);
		h->addWidget(r.line);
		h->addStretch(1);
		return w;
	}

	// Like the rest of the window, these take effect at Apply.
	QWidget *themeButtons(QComboBox *combo) {
		saveTheme = new QPushButton("Save");
		auto *saveAs = new QPushButton("Save as...");
		renameTheme = new QPushButton("Rename...");
		deleteTheme = new QPushButton("Delete");
		auto *dice = new QPushButton("Randomize");
		saveTheme->setToolTip("Keep the colors changed here in this theme.");
		saveAs->setToolTip("Keep the colors showing as a new theme of your own.");
		renameTheme->setToolTip("Give this theme another name.");
		deleteTheme->setToolTip("Delete this theme. Default is picked in its place.");
		dice->setToolTip("Random colors for every graph, in one scheme around the User color, with the "
						 "ones to watch kept red and orange. They're changes on this theme, so Save as keeps "
						 "them and picking a theme drops them.");
		for (auto *b : {saveTheme, saveAs, renameTheme, deleteTheme, dice})
			b->setAutoDefault(false);
		connect(saveTheme, &QPushButton::clicked, this, [this] {
			save_theme(d);
			refresh();
		});
		connect(saveAs, &QPushButton::clicked, this, [this] { nameTheme("Save theme as", QString(), false); });
		connect(renameTheme, &QPushButton::clicked, this,
			[this] { nameTheme("Rename theme", qs(theme_name(d)), true); });
		connect(deleteTheme, &QPushButton::clicked, this, [this] {
			const QString name = qs(theme_name(d));
			if (!askDelete(name))
				return;
			delete_theme(d);
			refresh();
		});
		connect(dice, &QPushButton::clicked, this, [this] {
			randomize_theme(d);
			refresh();
		});
		auto *w = new QWidget;
		auto *h = new QHBoxLayout(w);
		h->setContentsMargins(0, 0, 0, 0);
		h->addWidget(combo);
		h->addSpacing(12);
		for (auto *b : {saveTheme, saveAs, renameTheme, deleteTheme, dice})
			h->addWidget(b);
		h->addStretch(1);
		return w;
	}

	// asks again after a name that won't do, with the reason
	void nameTheme(const QString &title, QString name, bool rename) {
		for (;;) {
			bool ok = false;
			name = QInputDialog::getText(this, title, "Name:", QLineEdit::Normal, name, &ok);
			if (!ok)
				return;
			const std::string typed = name.toStdString();
			auto err = rename ? rename_theme(d, typed) : save_theme_as(d, typed);
			if (err.empty())
				break;
			QMessageBox::warning(this, title, qs(err));
		}
		refresh();
	}

	std::unique_ptr<ColorPicker> picker(Row &r) {
		if (!pick_open(d, r.id()))
			return nullptr;
		return std::make_unique<ColorPicker>(d, spokenName(r), [this] { refresh(); }, this);
	}

	void pickColor(Row &r) {
		if (auto pick = picker(r))
			pick->exec();
	}

	// an action rather than a setting, so it doesn't wait for Apply
	void resetHistory() {
		if (reset_deletes_saved(d) && !askReset())
			return;
		live_clear(*live);
		refresh();
		if (auto err = reset_history(d); !err.empty())
			QMessageBox::warning(this, "History wasn't reset", qs(err));
	}

	// a box still being typed in, which counts as a change before it's left
	bool typing() const {
		for (const auto &r : rows) {
			if (r->line && r->line->isModified())
				return true;
			if (r->spin && r->spin->text() != r->spin->textFromValue(r->spin->value()))
				return true;
		}
		return false;
	}

	void updateApply() {
		// the number boxes set their range before the button is made
		if (applyButton)
			applyButton->setEnabled(is_dirty(d) || typing());
	}

	// Alt+A clicks Apply with the focus left in the box, so nothing else
	// would take what was typed there.
	void takeTyped() {
		bool took = false;
		for (auto &r : rows) {
			if (r->line && r->line->isModified()) {
				r->line->setModified(false);
				set_text(d, r->id(), r->line->text().toStdString());
				took = true;
			}
			// a changed value refreshes through valueChanged
			if (r->spin)
				r->spin->interpretText();
		}
		if (took)
			refresh();
	}

	bool save() {
		takeTyped();
		if (!is_dirty(d))
			return true;
		auto err = apply(d);
		refresh();
		if (err.empty())
			return true;
		QMessageBox::warning(this, "Settings weren't saved", qs(err));
		return false;
	}

	void refresh() {
		for (auto &rp : rows) {
			Row &r = *rp;
			if (r.head) {
				const bool shown = row_state(d, r.id()).shown;
				r.head->setVisible(shown);
				r.rule->setVisible(shown);
				if (r.gap >= 0 && !(shown && r.sided))
					setMinHeight(r.grid, r.gap, shown ? HEAD_GAP : 0);
			}
			// the graph list's rows have a control and no label or revert
			if (!r.control)
				continue;
			auto st = row_state(d, r.id());
			r.canReset = st.can_reset;
			for (QWidget *w : {static_cast<QWidget *>(r.label), r.control, static_cast<QWidget *>(r.revert)})
				if (w)
					w->setVisible(st.shown);
			// greyed while a switch above it is off, but its revert still works
			for (QWidget *w : {static_cast<QWidget *>(r.label), r.control})
				if (w)
					w->setEnabled(st.counts);
			const QString tip = qs(st.tip);
			const QString text = qs(st.text);
			if (r.label) {
				setTip(r.label, tip);
				r.revert->setEnabled(st.can_reset);
				setTip(r.revert, st.can_reset ? "Back to the default." : "");
			}
			if (r.check) {
				const QSignalBlocker b(r.check);
				r.check->setChecked(st.on);
				setTip(r.check, tip);
			}
			if (r.slider) {
				const QSignalBlocker b(r.slider);
				r.slider->setValue(int(std::lround(st.slider * SLIDER_STEPS)));
				setTip(r.slider, tip);
			}
			if (r.spin) {
				const QSignalBlocker b(r.spin);
				r.spin->setValue(st.number);
				setTip(r.spin, tip);
			}
			if (r.combo) {
				const QSignalBlocker b(r.combo);
				std::vector<std::string> keys;
				for (auto &k : st.options)
					keys.push_back(std::string(k));
				if (keys != r.keys) {
					r.keys = keys;
					r.combo->clear();
					for (auto &l : st.option_labels)
						r.combo->addItem(qs(l));
				}
				for (size_t i = 0; i < r.keys.size(); i++) {
					const bool on = QString::fromStdString(r.keys[i]) == text;
					// a theme with changes on top says "Name *"
					r.combo->setItemText(int(i), on && !st.choice.empty() ? qs(st.choice) : qs(st.option_labels[i]));
					if (on)
						r.combo->setCurrentIndex(int(i));
				}
				setTip(r.combo, tip);
			}
			for (size_t i = 0; i < r.radios.size(); i++) {
				const QSignalBlocker b(r.radios[i]);
				r.radios[i]->setChecked(QString::fromStdString(r.keys[i]) == text);
				setTip(r.radios[i], tip);
			}
			if (r.line) {
				const QSignalBlocker b(r.line);
				r.line->setText(text);
				setTip(r.line, tip);
			}
			// a style sheet costs a restyle, and most refreshes change no color
			if (r.chip && text != r.chipColor) {
				r.chipColor = text;
				r.chip->setStyleSheet(QString("background-color: %1; border: 1px solid palette(mid);").arg(text));
			}
		}
		if (list)
			refreshList();
		orderSections();
		lineUpSections();
		if (saveTheme) {
			auto b = theme_buttons(d);
			saveTheme->setEnabled(b.save);
			renameTheme->setEnabled(b.rename);
			deleteTheme->setEnabled(b.remove);
		}
		updateApply();
	}
};

// Properties on the panel item sends SIGUSR1 when this window is open
// already. The handler only writes a byte, and the event loop does the rest.
class Wake final {
public:
	explicit Wake(std::function<void()> act) {
		if (pipe2(fds.data(), O_CLOEXEC | O_NONBLOCK) != 0)
			return;
		writeEnd = fds[1];
		struct sigaction sa{};
		sa.sa_handler = onSignal;
		sigemptyset(&sa.sa_mask);
		sa.sa_flags = SA_RESTART;
		if (sigaction(SIGUSR1, &sa, &before) != 0) {
			writeEnd = -1;
			return;
		}
		caught = true;
		notifier = std::make_unique<QSocketNotifier>(fds[0], QSocketNotifier::Read);
		QObject::connect(notifier.get(), &QSocketNotifier::activated, [this, act = std::move(act)] {
			std::array<char, 16> drain{};
			while (read(fds[0], drain.data(), drain.size()) > 0) {
			}
			act();
		});
	}
	Wake(const Wake &) = delete;
	Wake &operator=(const Wake &) = delete;
	Wake(Wake &&) = delete;
	Wake &operator=(Wake &&) = delete;
	~Wake() {
		if (caught)
			sigaction(SIGUSR1, &before, nullptr);
		writeEnd = -1;
		notifier.reset();
		for (const int fd : fds)
			if (fd >= 0)
				close(fd);
	}

private:
	static void onSignal(int) {
		const int saved = errno;
		if (writeEnd >= 0)
			(void)!write(writeEnd, "w", 1);
		errno = saved;
	}
	// a signal handler can reach nothing else
	static inline volatile std::sig_atomic_t writeEnd =
		-1; // NOLINT(cppcoreguidelines-avoid-non-const-global-variables)
	std::array<int, 2> fds{-1, -1};
	struct sigaction before{};
	bool caught = false;
	std::unique_ptr<QSocketNotifier> notifier;
};

// The few libxcb calls askActive makes, as libxcb declares them. Qt's xcb
// plugin has the library loaded, so it's looked up rather than linked.
struct XcbCookie {
	unsigned int sequence;
};
struct XcbAtomReply {
	std::uint8_t responseType, pad;
	std::uint16_t sequence;
	std::uint32_t length, atom;
};
// data points at the first screen, whose first field is its root window
struct XcbScreens {
	const std::uint32_t *data;
	int rem, index;
};
struct XcbClientMessage {
	std::uint8_t responseType, format;
	std::uint16_t sequence;
	std::uint32_t window, type;
	std::array<std::uint32_t, 5> data;
};

// A window on another workspace is unmapped there, and Qt doesn't ask to
// activate a window it thinks is unmapped. So this asks the way a task list
// does, as a pager, which the window manager follows to the window's
// workspace or brings it over, as it's set to. Nothing on Wayland.
void askActive(QWidget &w) {
	auto *x11 = qApp->nativeInterface<QNativeInterface::QX11Application>();
	void *lib = x11 ? dlopen("libxcb.so.1", RTLD_LAZY | RTLD_NOLOAD) : nullptr;
	if (!lib)
		return;
	auto *intern = reinterpret_cast<XcbCookie (*)(void *, std::uint8_t, std::uint16_t, const char *)>(
		dlsym(lib, "xcb_intern_atom"));
	auto *reply = reinterpret_cast<XcbAtomReply *(*)(void *, XcbCookie, void **)>(dlsym(lib, "xcb_intern_atom_reply"));
	auto *setup = reinterpret_cast<const void *(*)(void *)>(dlsym(lib, "xcb_get_setup"));
	auto *screens = reinterpret_cast<XcbScreens (*)(const void *)>(dlsym(lib, "xcb_setup_roots_iterator"));
	auto *send = reinterpret_cast<XcbCookie (*)(void *, std::uint8_t, std::uint32_t, std::uint32_t, const char *)>(
		dlsym(lib, "xcb_send_event"));
	auto *flush = reinterpret_cast<int (*)(void *)>(dlsym(lib, "xcb_flush"));
	void *c = x11->connection();
	if (intern && reply && setup && screens && send && flush && c) {
		static constexpr std::string_view name = "_NET_ACTIVE_WINDOW";
		XcbAtomReply *atom = reply(c, intern(c, 0, std::uint16_t(name.size()), name.data()), nullptr);
		const XcbScreens roots = screens(setup(c));
		if (atom && roots.data) {
			XcbClientMessage ev{};
			ev.responseType = 33; // ClientMessage
			ev.format = 32;
			ev.window = std::uint32_t(w.winId());
			ev.type = atom->atom;
			ev.data[0] = 2; // from a pager, which focus-stealing guards let through
			// SubstructureNotify and SubstructureRedirect, as EWMH asks
			send(c, 0, *roots.data, (1U << 19) | (1U << 20), reinterpret_cast<const char *>(&ev));
			flush(c);
		}
		std::free(atom); // NOLINT(cppcoreguidelines-no-malloc): libxcb mallocs its replies
	}
	dlclose(lib);
}

// Not minimized, on top, and focused, with a picker or question that's open
// over it still on top of it.
void comeForward(QWidget &w) {
	w.setWindowState((w.windowState() & ~Qt::WindowMinimized) | Qt::WindowActive);
	w.raise();
	w.activateWindow();
	QWidget *top = QApplication::activeModalWidget();
	if (top && top != &w) {
		top->raise();
		top->activateWindow();
	}
	askActive(top ? *top : w);
}

// Qt reads argc and argv for as long as the app lives, and the platform
// before the app is made, so both come from a base built first.
struct AppArgs {
	explicit AppArgs(const QByteArray &platform) {
		if (!platform.isEmpty())
			qputenv("QT_QPA_PLATFORM", platform);
	}
	int argc = 1;
	std::array<char, sizeof("tukzmon-settings")> name{"tukzmon-settings"};
	std::array<char *, 2> argv{name.data(), nullptr};
};

// One per call. The test hooks pass an offscreen platform.
class App final : AppArgs, public QApplication {
public:
	explicit App(const QByteArray &platform = {}) : AppArgs(platform), QApplication(argc, argv.data()) {}
};

void openSized(Window &w) {
	// a page not shown yet keeps the size it had before its dropdowns were
	// filled, which squeezed the graph list
	for (auto *c : w.findChildren<QWidget *>())
		c->updateGeometry();
	QSize size = w.sizeHint().expandedTo(QSize(560, 420));
	if (const QScreen *screen = w.screen())
		size = fitSize(size, screen->availableGeometry(), QMargins());
	w.resize(size);
}

} // namespace

void say_unreadable(rust::Str text) {
	const App app;
	QApplication::setApplicationName("tukzmon-settings");
	QMessageBox::critical(nullptr, "Settings can't be read",
		QString::fromUtf8(text.data(), int(text.size())) + "\n\nNothing was changed.");
}

std::int32_t run_dialog(Dialog &d, rust::Str snapshot) {
	App app;
	QApplication::setApplicationName("tukzmon-settings");
	TipWrap tipWrap;
	app.installEventFilter(&tipWrap);
	tukzmon::App::setStyle(new BoxStyle);
	Window w(d);
	openSized(w);
	if (snapshot.empty()) {
		const Wake wake([&w] { comeForward(w); });
		return w.exec() == QDialog::Accepted ? 0 : 1;
	}

	// one picture per tab with rows, as base-1.png, base-2.png, and so on, then
	// the color picker
	QString base = QString::fromUtf8(snapshot.data(), int(snapshot.size()));
	if (base.endsWith(".png"))
		base.chop(4);
	w.show();
	int n = 0;
	for (int i = 0; i < w.tabCount(); i++) {
		if (!w.isLeaf(i))
			continue;
		w.select(i);
		tukzmon::App::processEvents();
		if (!w.grab().save(QString("%1-%2.png").arg(base).arg(++n)))
			return 3;
	}
	if (auto pick = w.firstPicker()) {
		pick->show();
		tukzmon::App::processEvents();
		const bool saved = pick->grab().save(QString("%1-%2.png").arg(base).arg(++n));
		pick->reject();
		if (!saved)
			return 3;
	}
	return 0;
}

rust::Vec<rust::String> greyed_rows(Dialog &d) {
	const App app("offscreen");
	const Window w(d);
	rust::Vec<rust::String> ids;
	for (auto &id : w.greyed())
		ids.push_back(rust::String(id));
	return ids;
}

rust::Vec<std::int32_t> graph_list_places(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	w.show();
	tukzmon::App::processEvents();
	rust::Vec<std::int32_t> out;
	for (const int v : w.listPlaces())
		out.push_back(v);
	return out;
}

namespace {
TabShown shownNow(Window &w, const std::string &label) {
	TabShown t;
	t.label = rust::String(label);
	for (auto &id : w.shownRows())
		t.rows.push_back(rust::String(id));
	for (auto &line : w.dimLines())
		t.dim.push_back(rust::String(line));
	return t;
}

void eachTab(Window &w, rust::Vec<TabShown> &out) {
	for (int i = 0; i < w.tabCount(); i++) {
		w.select(i);
		QApplication::processEvents();
		out.push_back(shownNow(w, w.tabText(i)));
	}
}
} // namespace

rust::Vec<TabShown> tabs_shown(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	w.show();
	rust::Vec<TabShown> out;
	eachTab(w, out);
	return out;
}

rust::Vec<TabShown> tabs_after_move(Dialog &d, rust::Str pick, std::size_t from, std::size_t to) {
	const App app("offscreen");
	Window w(d);
	w.show();
	const QString want = QString::fromUtf8(pick.data(), int(pick.size()));
	for (int i = 0; i < w.tabCount(); i++)
		if (QString::fromStdString(w.tabText(i)).trimmed() == want)
			w.select(i);
	tukzmon::App::processEvents();
	w.moveGraph(int(from), int(to));
	tukzmon::App::processEvents();
	rust::Vec<TabShown> out;
	out.push_back(shownNow(w, w.selectedText()));
	eachTab(w, out);
	return out;
}

rust::Vec<rust::String> sections_after_move(Dialog &d, std::size_t from, std::size_t to) {
	const App app("offscreen");
	Window w(d);
	w.show();
	for (int i = 0; i < w.tabCount(); i++)
		if (QString::fromStdString(w.tabText(i)).trimmed() == "Colors")
			w.select(i);
	tukzmon::App::processEvents();
	w.moveGraph(int(from), int(to));
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	for (auto &s : w.sectionPlaces())
		out.push_back(rust::String(s));
	return out;
}

rust::Vec<rust::String> tabs_by_key(Dialog &d, bool back, bool fromPage, bool pageKeys) {
	const App app("offscreen");
	Window w(d);
	w.show();
	w.activateWindow();
	tukzmon::App::processEvents();
	if (fromPage)
		if (QWidget *box = w.firstBox(true))
			box->setFocus();
	rust::Vec<rust::String> out;
	for (int i = 0; i < w.tabCount(); i++) {
		out.push_back(rust::String(QString::fromStdString(w.selectedText()).trimmed().toStdString()));
		QWidget *to = QApplication::focusWidget() ? QApplication::focusWidget() : &w;
		int key = back ? Qt::Key_Backtab : Qt::Key_Tab;
		if (pageKeys)
			key = back ? Qt::Key_PageUp : Qt::Key_PageDown;
		QKeyEvent down(QEvent::KeyPress, key,
			back && !pageKeys ? Qt::ControlModifier | Qt::ShiftModifier : Qt::ControlModifier);
		// Qt runs app filters only on the thread that made the first
		// QApplication, and each test has a thread of its own
		if (!w.eventFilter(to, &down))
			QApplication::sendEvent(to, &down);
		tukzmon::App::processEvents();
	}
	return out;
}

rust::Vec<rust::String> section_tops(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	w.show();
	for (int i = 0; i < w.tabCount(); i++)
		if (QString::fromStdString(w.tabText(i)).trimmed() == "Colors")
			w.select(i);
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	for (auto &s : w.sectionTops())
		out.push_back(rust::String(s));
	return out;
}

rust::Vec<rust::String> row_spots(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	openSized(w);
	w.show();
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	for (auto &s : w.rowSpots())
		out.push_back(rust::String(s));
	return out;
}

rust::Vec<rust::String> colors_row_spots(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	openSized(w);
	w.show();
	for (int i = 0; i < w.tabCount(); i++)
		if (QString::fromStdString(w.tabText(i)).trimmed() == "Colors")
			w.select(i);
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	for (auto &s : w.rowSpots())
		out.push_back(rust::String(s));
	return out;
}

rust::Vec<rust::String> list_cells(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	openSized(w);
	w.show();
	for (int i = 0; i < w.tabCount(); i++)
		if (QString::fromStdString(w.tabText(i)).trimmed() == "Graphs")
			w.select(i);
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	for (auto &s : w.listCells())
		out.push_back(rust::String(s));
	return out;
}

rust::Vec<rust::String> bottom_buttons(Dialog &d) {
	const App app("offscreen");
	const Window w(d);
	rust::Vec<rust::String> out;
	for (auto &t : w.buttonTexts())
		out.push_back(rust::String(t));
	return out;
}

rust::Vec<rust::String> enter_on(Dialog &d, rust::Str button) {
	const App app("offscreen");
	Window w(d);
	w.show();
	w.activateWindow();
	tukzmon::App::processEvents();
	QString name = QString::fromUtf8(button.data(), int(button.size()));
	std::unique_ptr<ColorPicker> pick;
	QDialog *top = &w;
	if (name.startsWith("picker ")) {
		pick = w.firstPicker();
		if (!pick)
			return {};
		pick->show();
		top = pick.get();
		name.remove(0, 7);
	}
	QWidget *to = nullptr;
	int clicks = 0;
	auto find = [&] {
		for (auto *b : top->findChildren<QPushButton *>())
			if (QString(b->text()).remove(QChar('&')) == name && b->isVisible()) {
				QObject::connect(b, &QPushButton::clicked, &w, [&clicks] { clicks++; });
				to = b;
				return;
			}
	};
	if (pick) {
		tukzmon::App::processEvents();
		find();
	}
	for (int i = 0; i < w.tabCount() && !to && !pick; i++) {
		w.select(i);
		tukzmon::App::processEvents();
		if (name.isEmpty())
			to = w.firstBox(true);
		else
			find();
	}
	if (!to)
		return {};
	to->setFocus();
	QKeyEvent down(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
	// the app filter runs only on the first QApplication's thread, as in tabs_by_key
	if (!w.eventFilter(to, &down))
		QApplication::sendEvent(to, &down);
	tukzmon::App::processEvents();
	const char *closed = top->result() == QDialog::Accepted ? "ok" : "cancel";
	rust::Vec<rust::String> out;
	out.push_back(rust::String(std::to_string(clicks)));
	out.push_back(rust::String(top->isVisible() ? "open" : closed));
	return out;
}

rust::Vec<rust::String> list_head_tips(Dialog &d) {
	const App app("offscreen");
	const Window w(d);
	rust::Vec<rust::String> out;
	for (const char *head : {"Width %", "Sample %", "Time axis", "Vertical scale"})
		for (auto *l : w.findChildren<QLabel *>())
			// General's Time axis heading is bold, and isn't the list's
			if (l->text() == head && !l->font().bold())
				out.push_back(rust::String(std::string(head) + ": " + l->toolTip().toStdString()));
	return out;
}

namespace {
void press(QWidget *w, int key, const QString &text = QString()) {
	QKeyEvent down(QEvent::KeyPress, key, Qt::NoModifier, text);
	QKeyEvent up(QEvent::KeyRelease, key, Qt::NoModifier, text);
	QApplication::sendEvent(w, &down);
	QApplication::sendEvent(w, &up);
}
} // namespace

rust::Vec<bool> apply_while_typing(Dialog &d, bool number) {
	const App app("offscreen");
	Window w(d);
	QWidget *box = w.firstBox(number);
	if (!box)
		return {};
	rust::Vec<bool> out;
	out.push_back(w.applyOn());
	// the last digit typed over with another, which stays in range
	auto *spin = qobject_cast<QDoubleSpinBox *>(box);
	const QString text = spin ? spin->text() : static_cast<QLineEdit *>(box)->text();
	const QChar digit = text.back() == '1' ? '2' : '1';
	press(box, Qt::Key_End);
	press(box, Qt::Key_Backspace);
	press(box, digit == '1' ? Qt::Key_1 : Qt::Key_2, digit);
	out.push_back(w.applyOn());
	w.commitTyped();
	out.push_back(is_dirty(d));
	return out;
}

rust::Vec<bool> teardown_while_typing(Dialog &d) {
	const App app("offscreen");
	auto w = std::make_unique<Window>(d);
	w->show();
	w->activateWindow();
	QWidget *box = w->firstBox(false);
	for (int i = 0; box && !box->isVisible() && i < w->tabCount(); i++) {
		w->select(i);
		tukzmon::App::processEvents();
	}
	if (!box || !box->isVisible())
		return {};
	box->setFocus();
	tukzmon::App::processEvents();
	press(box, Qt::Key_End);
	press(box, Qt::Key_Backspace);
	rust::Vec<bool> out;
	out.push_back(box->hasFocus());
	out.push_back(w->applyOn());
	w.reset();
	out.push_back(is_dirty(d));
	return out;
}

rust::Vec<rust::String> control_names(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	rust::Vec<rust::String> out;
	for (QWidget *c : w.findChildren<QWidget *>()) {
		if (!(c->focusPolicy() & Qt::TabFocus))
			continue;
		// a box's own line edit is the box
		bool inner = false;
		for (QWidget *up = c->parentWidget(); up && up != &w; up = up->parentWidget())
			inner |= qobject_cast<QAbstractSpinBox *>(up) || qobject_cast<QComboBox *>(up);
		if (inner)
			continue;
		const QAccessibleInterface *a = QAccessible::queryAccessibleInterface(c);
		QString line = QString(c->metaObject()->className()) + "|";
		if (a) {
			line += a->text(QAccessible::Name);
			for (const auto &[by, how] : a->relations(QAccessible::Label))
				line += "|" + by->text(QAccessible::Name);
		}
		out.push_back(rust::String(line.toStdString()));
	}
	if (auto pick = w.firstPicker()) {
		out.push_back(rust::String("title|" + pick->windowTitle().toStdString()));
		pick->reject();
	}
	return out;
}

namespace {
// close to the dark themes where the empty checkbox went missing
QPalette testPalette(bool dark) {
	QPalette pal = QApplication::palette();
	if (dark) {
		pal.setColor(QPalette::Window, QColor("#2b2b2b"));
		pal.setColor(QPalette::WindowText, QColor("#dcdcdc"));
		pal.setColor(QPalette::Base, QColor("#2f2f2f"));
		pal.setColor(QPalette::Text, QColor("#dcdcdc"));
		pal.setColor(QPalette::Button, QColor("#353535"));
	}
	return pal;
}
} // namespace

rust::Vec<rust::String> page_shades(Dialog &d, bool dark) {
	const App app("offscreen");
	tukzmon::App::setStyle(new BoxStyle);
	const QPalette pal = testPalette(dark);
	tukzmon::App::setPalette(pal);
	Window w(d);
	w.resize(w.sizeHint().expandedTo(QSize(560, 420)));
	w.show();
	rust::Vec<rust::String> out;
	out.push_back(rust::String(pal.color(QPalette::Window).name().toStdString()));
	for (int i = 0; i < w.tabCount(); i++) {
		w.select(i);
		tukzmon::App::processEvents();
		const auto [in, tabs, rest] = w.shadesNow();
		out.push_back(rust::String(QString::fromStdString(w.tabText(i)).trimmed().toStdString() + " " +
			in.name().toStdString() + " " + tabs.name().toStdString() + " " + rest.name().toStdString()));
	}
	return out;
}

rust::Vec<std::int32_t> chip_restyles(Dialog &d, rust::Str color) {
	const App app("offscreen");
	Window w(d);
	rust::Vec<std::int32_t> out;
	for (const int n : w.chipRestyles(std::string(color)))
		out.push_back(n);
	return out;
}

rust::Vec<std::int32_t> refresh_churn(Dialog &d, rust::Str id, double number) {
	const App app("offscreen");
	Window w(d);
	w.show();
	tukzmon::App::processEvents();
	rust::Vec<std::int32_t> out;
	for (const int n : w.refreshChurn(std::string(id), number))
		out.push_back(n);
	return out;
}

rust::Vec<double> refresh_micros(Dialog &d, std::size_t times) {
	const App app("offscreen");
	Window w(d);
	w.show();
	tukzmon::App::processEvents();
	rust::Vec<double> out;
	for (const double v : w.refreshTimes(int(times)))
		out.push_back(v);
	return out;
}

double empty_box_contrast(bool dark) {
	const App app("offscreen");
	tukzmon::App::setStyle(new BoxStyle);
	const QPalette pal = testPalette(dark);
	tukzmon::App::setPalette(pal);
	QCheckBox box;
	const QImage img = box.grab().toImage();
	auto lum = [](QColor c) {
		auto lin = [](double v) { return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4); };
		return (0.2126 * lin(c.redF())) + (0.7152 * lin(c.greenF())) + (0.0722 * lin(c.blueF()));
	};
	const double back = lum(pal.color(QPalette::Window));
	double best = 1;
	for (int y = 0; y < img.height(); y++)
		for (int x = 0; x < img.width(); x++) {
			const double l = lum(img.pixelColor(x, y));
			best = std::max(best, (std::max(l, back) + 0.05) / (std::min(l, back) + 0.05));
		}
	return best;
}

rust::Vec<rust::String> button_gaps(Dialog &d) {
	const App app("offscreen");
	tukzmon::App::setStyle(new BoxStyle);
	Window w(d);
	QMessageBox box(QMessageBox::Question, "Reset history", "Reset history?", QMessageBox::Yes | QMessageBox::No);
	rust::Vec<rust::String> out;
	auto *style = tukzmon::App::style();
	for (QWidget *top : std::initializer_list<QWidget *>{&w, &box}) {
		top->show();
		tukzmon::App::processEvents();
		for (auto *b : top->findChildren<QPushButton *>()) {
			if (b->text().isEmpty())
				continue;
			QStyleOptionButton opt;
			opt.initFrom(b);
			opt.text = b->text();
			if (b->autoDefault())
				opt.features |= QStyleOptionButton::AutoDefaultButton;
			const QRect inner = style->subElementRect(QStyle::SE_PushButtonContents, &opt, b);
			const QSize text = b->fontMetrics().size(Qt::TextShowMnemonic, b->text());
			out.push_back(
				rust::String(b->text().toStdString() + " " + std::to_string((inner.width() - text.width()) / 2) + " " +
					std::to_string((inner.height() - text.height()) / 2)));
		}
	}
	return out;
}

rust::Vec<rust::String> button_heights(Dialog &d) {
	const App app("offscreen");
	tukzmon::App::setStyle(new BoxStyle);
	Window w(d);
	openSized(w);
	w.show();
	rust::Vec<rust::String> out;
	std::vector<const QPushButton *> seen;
	auto take = [&](const QWidget &top) {
		for (auto *b : top.findChildren<QPushButton *>()) {
			if (b->text().isEmpty() || !b->isVisible() || std::ranges::find(seen, b) != seen.end())
				continue;
			seen.push_back(b);
			out.push_back(rust::String(b->text().toStdString() + " " + std::to_string(b->height())));
		}
	};
	for (int i = 0; i < w.tabCount(); i++) {
		w.select(i);
		tukzmon::App::processEvents();
		take(w);
	}
	if (auto pick = w.firstPicker()) {
		pick->show();
		tukzmon::App::processEvents();
		take(*pick);
	}
	QMessageBox box(QMessageBox::Question, "Reset history", "Reset history?", QMessageBox::Yes | QMessageBox::No);
	box.show();
	tukzmon::App::processEvents();
	take(box);
	return out;
}

rust::Vec<rust::String> page_heads(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	openSized(w);
	w.show();
	rust::Vec<rust::String> out;
	for (int i = 0; i < w.tabCount(); i++) {
		w.select(i);
		tukzmon::App::processEvents();
		QStringList heads;
		for (auto *l : w.findChildren<QLabel *>())
			if (l->isVisible() && l->font().bold())
				heads << l->text();
		const auto rules = std::ranges::count_if(w.findChildren<QFrame *>(),
			[](const QFrame *f) { return f->isVisible() && f->frameShape() == QFrame::HLine; });
		out.push_back(rust::String(QString::fromStdString(w.tabText(i)).trimmed().toStdString() + "|" +
			heads.join(";").toStdString() + "|" + std::to_string(rules)));
	}
	return out;
}

rust::Vec<rust::String> question_defaults(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	rust::Vec<rust::String> out;
	QTimer answer;
	QObject::connect(&answer, &QTimer::timeout, [&out] {
		auto *box = qobject_cast<QMessageBox *>(QApplication::activeModalWidget());
		if (!box || !box->isVisible())
			return;
		QString picked = "none";
		for (auto *b : box->findChildren<QPushButton *>())
			if (b->isDefault())
				picked = b->text().remove('&');
		out.push_back(rust::String((box->windowTitle() + " " + picked).toStdString()));
		box->button(QMessageBox::No)->click();
	});
	answer.start(10);
	const bool deleted = w.askDelete("Mine");
	const bool reset = w.askReset();
	out.push_back(rust::String(std::string("yes ") + (deleted ? "1" : "0") + (reset ? "1" : "0")));
	return out;
}

rust::Vec<rust::String> tab_walk(Dialog &d, bool colors, std::size_t from, std::size_t to) {
	const App app("offscreen");
	Window w(d);
	w.show();
	w.activateWindow();
	for (int i = 0; i < w.tabCount(); i++)
		if (QString::fromStdString(w.tabText(i)).trimmed() == (colors ? "Colors" : "Graphs"))
			w.select(i);
	tukzmon::App::processEvents();
	w.moveGraph(int(from), int(to));
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	QWidget *start = w.firstStop(colors);
	if (!start)
		return out;
	start->setFocus(Qt::TabFocusReason);
	tukzmon::App::processEvents();
	for (int i = 0; i < 5000; i++) {
		QWidget *at = QApplication::focusWidget();
		const std::string group = at ? w.groupOf(at) : std::string();
		if (group.empty() || (i > 0 && at == start))
			break;
		if (out.empty() || std::string(out.back()) != group)
			out.push_back(rust::String(group));
		QKeyEvent tab(QEvent::KeyPress, Qt::Key_Tab, Qt::NoModifier);
		QApplication::sendEvent(at, &tab);
		tukzmon::App::processEvents();
	}
	return out;
}

rust::Vec<rust::String> slider_clicks(Dialog &d) {
	const App app("offscreen");
	tukzmon::App::setStyle(new BoxStyle);
	Window w(d);
	openSized(w);
	w.show();
	const Row *row = w.firstSlider();
	if (!row)
		return {};
	TrackSlider *s = row->slider;
	for (int i = 0; i < w.tabCount() && !s->isVisible(); i++) {
		w.select(i);
		tukzmon::App::processEvents();
	}
	rust::Vec<rust::String> out;
	auto say = [&out](const std::string &what, int v) { out.push_back(rust::String(what + " " + std::to_string(v))); };
	const QRect handle = s->part(QStyle::SC_SliderHandle);
	say("handle", handle.height());
	say("box", row->spin->height());
	// where the handle's middle sits at t, 0 to 1 along the track
	auto at = [s](double t) {
		const QRect g = s->part(QStyle::SC_SliderGroove);
		const int hw = s->part(QStyle::SC_SliderHandle).width();
		const int x = g.x() + (hw / 2) + int(std::lround(t * (g.width() - hw)));
		return QPointF(QPoint(x, s->height() / 2));
	};
	auto mouse = [s](QEvent::Type type, QPointF p, Qt::MouseButton b, Qt::MouseButtons held) {
		QMouseEvent e(type, p, s->mapToGlobal(p), b, held, Qt::NoModifier);
		QApplication::sendEvent(s, &e);
		tukzmon::App::processEvents();
	};
	auto click = [&](QPointF p) {
		mouse(QEvent::MouseButtonPress, p, Qt::LeftButton, Qt::LeftButton);
		mouse(QEvent::MouseButtonRelease, p, Qt::LeftButton, Qt::NoButton);
	};
	click(at(0.8));
	say("jump", s->value());
	const QRect now = s->part(QStyle::SC_SliderHandle);
	click(QPointF(now.left() + 1, now.center().y()));
	say("grab", s->value());
	mouse(QEvent::MouseButtonPress, at(0.2), Qt::LeftButton, Qt::LeftButton);
	mouse(QEvent::MouseMove, at(0.5), Qt::NoButton, Qt::LeftButton);
	mouse(QEvent::MouseButtonRelease, at(0.5), Qt::LeftButton, Qt::NoButton);
	say("drag", s->value());
	return out;
}

rust::Vec<rust::String> window_fit(Dialog &d, rust::Str screens) {
	const App app("offscreen:configfile=" + QByteArray(screens.data(), qsizetype(screens.size())));
	tukzmon::App::setStyle(new BoxStyle);
	Window w(d);
	openSized(w);
	w.show();
	tukzmon::App::processEvents();
	auto rect = [](const char *what, const QRect &r) {
		return rust::String(std::string(what) + " " + std::to_string(r.x()) + " " + std::to_string(r.y()) + " " +
			std::to_string(r.width()) + " " + std::to_string(r.height()));
	};
	rust::Vec<rust::String> out;
	out.push_back(rect("room", w.screen()->availableGeometry()));
	out.push_back(rect("frame", w.frameGeometry()));
	for (int i = 0; i < w.tabCount(); i++) {
		if (!w.isLeaf(i))
			continue;
		w.select(i);
		tukzmon::App::processEvents();
		out.push_back(rust::String(QString::fromStdString(w.tabText(i)).trimmed().toStdString() + " " + w.scrollNow()));
	}
	return out;
}

rust::Vec<rust::String> squeezed(Dialog &d, rust::Str screens) {
	const App app("offscreen:configfile=" + QByteArray(screens.data(), qsizetype(screens.size())));
	tukzmon::App::setStyle(new BoxStyle);
	Window w(d);
	openSized(w);
	w.show();
	rust::Vec<rust::String> out;
	for (int i = 0; i < w.tabCount(); i++) {
		if (!w.isLeaf(i))
			continue;
		w.select(i);
		tukzmon::App::processEvents();
		for (const auto *c : w.findChildren<QComboBox *>())
			if (c->isVisible() && c->width() < c->sizeHint().width())
				out.push_back(rust::String(QString::fromStdString(w.tabText(i)).trimmed().toStdString() + ": " +
					c->accessibleName().toStdString() + " " + std::to_string(c->width()) + " of " +
					std::to_string(c->sizeHint().width())));
	}
	return out;
}

rust::Vec<rust::String> woken_by_signal(Dialog &d) {
	const App app("offscreen");
	Window w(d);
	w.show();
	tukzmon::App::processEvents();
	w.setWindowState(Qt::WindowMinimized);
	tukzmon::App::processEvents();
	rust::Vec<rust::String> out;
	auto handler = [] {
		struct sigaction now{};
		sigaction(SIGUSR1, nullptr, &now);
		return now.sa_handler == SIG_DFL ? std::string("default") : std::string("caught");
	};
	{
		int woken = 0;
		const Wake wake([&] {
			woken++;
			comeForward(w);
		});
		out.push_back(rust::String("while open " + handler()));
		std::raise(SIGUSR1);
		QElapsedTimer t;
		t.start();
		while (!woken && t.elapsed() < 2000)
			tukzmon::App::processEvents(QEventLoop::WaitForMoreEvents, 50);
		out.push_back(rust::String("woken " + std::to_string(woken)));
		out.push_back(rust::String(w.isMinimized() ? "minimized" : "shown"));
	}
	out.push_back(rust::String("after " + handler()));
	return out;
}

} // namespace tukzmon
